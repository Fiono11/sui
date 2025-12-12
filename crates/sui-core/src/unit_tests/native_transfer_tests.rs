// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::crypto::get_account_key_pair;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::execution_status::{ExecutionFailureStatus, ExecutionStatus};
use sui_types::gas_coin::GasCoin;
use sui_types::object::Object;
use sui_types::transaction::{TransactionData, TransactionDataAPI, VerifiedTransaction};
use sui_types::utils::to_sender_signed_transaction;

use crate::authority::ExecutionEnv;
use crate::authority::test_authority_builder::TestAuthorityBuilder;
use crate::execution_scheduler::SchedulingSource;
use crate::test_utils::send_and_confirm_transaction;

/// Get a protocol config with execution_version 2 (v2 execution engine)
/// Protocol version 31 has execution_version 2, but we need to override the congestion control mode
fn protocol_config_v2() -> ProtocolConfig {
    use sui_protocol_config::{ExecutionTimeEstimateParams, PerObjectCongestionControlMode};
    let mut config = ProtocolConfig::get_for_version(ProtocolVersion::new(31), Chain::Unknown);
    // Set congestion control mode to ExecutionTimeEstimate (required for v2 execution engine)
    config.set_per_object_congestion_control_mode_for_testing(
        PerObjectCongestionControlMode::ExecutionTimeEstimate(ExecutionTimeEstimateParams {
            target_utilization: 50,
            allowed_txn_cost_overage_burst_limit_us: 500_000,
            randomness_scalar: 20,
            max_estimate_us: 1_500_000,
            stored_observations_num_included_checkpoints: 10,
            stored_observations_limit: 180,
            stake_weighted_median_threshold: 3334,
            default_none_duration_for_new_keys: true,
            observations_chunk_size: Some(18),
        }),
    );
    config
}

#[tokio::test]
async fn test_native_transfer_success() {
    let (sender, sender_key) = get_account_key_pair();
    let recipient = SuiAddress::random_for_testing_only();

    // Create a gas coin with 1000 MIST
    let coin_id = ObjectID::random();
    let coin_value = 1000;
    let gas_coin = GasCoin::new(coin_id, coin_value);
    let coin_object = Object::new_move(
        gas_coin.to_object(sui_types::base_types::SequenceNumber::from_u64(1)),
        sui_types::object::Owner::AddressOwner(sender),
        sui_types::base_types::TransactionDigest::ZERO,
    );
    let coin_ref = coin_object.compute_object_reference();

    let state = TestAuthorityBuilder::new()
        .with_protocol_config(protocol_config_v2())
        .with_starting_objects(&[coin_object])
        .build()
        .await;

    // Create native transfer transaction
    let transfer_amount = 500;
    let tx_data =
        TransactionData::new_native_transfer(sender, coin_ref, recipient, transfer_amount);
    let signed_tx = to_sender_signed_transaction(tx_data, &sender_key);

    // Execute the transaction
    let (_cert, effects) = send_and_confirm_transaction(&state, None, signed_tx)
        .await
        .unwrap();

    // Verify execution succeeded
    assert!(effects.status().is_ok());

    // Verify gas was not charged (unmetered)
    let gas_cost = effects.gas_cost_summary();
    assert_eq!(
        gas_cost.net_gas_usage(),
        0,
        "NativeTransfer should not charge gas"
    );

    // Verify the coin was updated correctly
    let updated_coin = state.get_object(&coin_id).await.unwrap();
    let updated_gas_coin = GasCoin::try_from(&updated_coin).unwrap();
    assert_eq!(
        updated_gas_coin.value(),
        coin_value - transfer_amount,
        "Source coin should have amount deducted"
    );

    // Verify new coin was created for recipient
    let created_objects = effects.created();
    assert_eq!(created_objects.len(), 1, "Should create one new coin");
    let new_coin_id = created_objects[0].0.0;
    let new_coin = state.get_object(&new_coin_id).await.unwrap();
    let new_gas_coin = GasCoin::try_from(&new_coin).unwrap();
    assert_eq!(
        new_gas_coin.value(),
        transfer_amount,
        "New coin should have transfer amount"
    );
    assert_eq!(
        new_coin.owner,
        sui_types::object::Owner::AddressOwner(recipient),
        "New coin should be owned by recipient"
    );
}

#[tokio::test]
async fn test_native_transfer_insufficient_balance() {
    let (sender, sender_key) = get_account_key_pair();
    let recipient = SuiAddress::random_for_testing_only();

    // Create a gas coin with only 100 MIST
    let coin_id = ObjectID::random();
    let coin_value = 100;
    let gas_coin = GasCoin::new(coin_id, coin_value);
    let coin_object = Object::new_move(
        gas_coin.to_object(sui_types::base_types::SequenceNumber::from_u64(1)),
        sui_types::object::Owner::AddressOwner(sender),
        sui_types::base_types::TransactionDigest::ZERO,
    );
    let coin_ref = coin_object.compute_object_reference();

    let state = TestAuthorityBuilder::new()
        .with_protocol_config(protocol_config_v2())
        .with_starting_objects(&[coin_object])
        .build()
        .await;

    // Try to transfer more than available
    let transfer_amount = 500; // More than coin_value
    let tx_data =
        TransactionData::new_native_transfer(sender, coin_ref, recipient, transfer_amount);
    let signed_tx = to_sender_signed_transaction(tx_data, &sender_key);

    // Execute should fail
    let result = send_and_confirm_transaction(&state, None, signed_tx).await;
    // send_and_confirm_transaction returns Ok even for failed transactions,
    // we need to check the effects status
    if let Ok((_cert, effects)) = result {
        assert!(
            !effects.status().is_ok(),
            "Transaction should fail with insufficient balance"
        );
        match effects.status() {
            ExecutionStatus::Failure {
                error: ExecutionFailureStatus::InsufficientCoinBalance,
                ..
            } => {
                // Expected failure
            }
            status => {
                panic!(
                    "Should fail with InsufficientCoinBalance, got: {:?}",
                    status
                );
            }
        }
    } else {
        panic!(
            "Unexpected error from send_and_confirm_transaction: {:?}",
            result
        );
    }
}

#[tokio::test]
async fn test_native_transfer_wrong_owner() {
    let (sender, sender_key) = get_account_key_pair();
    let (other_owner, _) = get_account_key_pair();
    let recipient = SuiAddress::random_for_testing_only();

    // Create a gas coin owned by someone else
    let coin_id = ObjectID::random();
    let coin_value = 1000;
    let gas_coin = GasCoin::new(coin_id, coin_value);
    let coin_object = Object::new_move(
        gas_coin.to_object(sui_types::base_types::SequenceNumber::from_u64(1)),
        sui_types::object::Owner::AddressOwner(other_owner), // Not owned by sender
        sui_types::base_types::TransactionDigest::ZERO,
    );
    let coin_ref = coin_object.compute_object_reference();

    let state = TestAuthorityBuilder::new()
        .with_protocol_config(protocol_config_v2())
        .with_starting_objects(&[coin_object])
        .build()
        .await;

    // Try to transfer with wrong owner
    let transfer_amount = 500;
    let tx_data = TransactionData::new_native_transfer(
        sender, // Different from coin owner
        coin_ref,
        recipient,
        transfer_amount,
    );
    let signed_tx = to_sender_signed_transaction(tx_data, &sender_key);

    // Execute should fail
    let result = send_and_confirm_transaction(&state, None, signed_tx).await;
    assert!(result.is_err(), "Should fail with wrong owner");

    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("owner") || err.to_string().contains("owned"),
        "Error should mention ownership: {}",
        err
    );
}

#[tokio::test]
async fn test_native_transfer_zero_amount() {
    let (sender, _sender_key) = get_account_key_pair();
    let recipient = SuiAddress::random_for_testing_only();

    // Create a gas coin
    let coin_id = ObjectID::random();
    let coin_value = 1000;
    let gas_coin = GasCoin::new(coin_id, coin_value);
    let coin_object = Object::new_move(
        gas_coin.to_object(sui_types::base_types::SequenceNumber::from_u64(1)),
        sui_types::object::Owner::AddressOwner(sender),
        sui_types::base_types::TransactionDigest::ZERO,
    );
    let coin_ref = coin_object.compute_object_reference();

    let state = TestAuthorityBuilder::new()
        .with_protocol_config(protocol_config_v2())
        .with_starting_objects(&[coin_object])
        .build()
        .await;

    // Try to transfer zero amount
    let transfer_amount = 0;
    let tx_data =
        TransactionData::new_native_transfer(sender, coin_ref, recipient, transfer_amount);

    // Should fail validation
    let epoch_store = state.epoch_store_for_testing();
    let validity_result = tx_data.validity_check(epoch_store.protocol_config());
    assert!(
        validity_result.is_err(),
        "Should fail validation for zero amount"
    );
}

#[tokio::test]
async fn test_native_transfer_fast_path() {
    let (sender, sender_key) = get_account_key_pair();
    let recipient = SuiAddress::random_for_testing_only();

    // Create a gas coin
    let coin_id = ObjectID::random();
    let coin_value = 1000;
    let gas_coin = GasCoin::new(coin_id, coin_value);
    let coin_object = Object::new_move(
        gas_coin.to_object(sui_types::base_types::SequenceNumber::from_u64(1)),
        sui_types::object::Owner::AddressOwner(sender),
        sui_types::base_types::TransactionDigest::ZERO,
    );
    let coin_ref = coin_object.compute_object_reference();

    let state = TestAuthorityBuilder::new()
        .with_protocol_config(protocol_config_v2())
        .with_starting_objects(&[coin_object])
        .build()
        .await;

    // Create native transfer transaction
    let transfer_amount = 500;
    let tx_data =
        TransactionData::new_native_transfer(sender, coin_ref, recipient, transfer_amount);
    let signed_tx = to_sender_signed_transaction(tx_data, &sender_key);
    let cert = VerifiedExecutableTransaction::new_from_quorum_execution(
        VerifiedTransaction::new_unchecked(signed_tx),
        0,
    );

    // Execute via fast path
    let (effects, _) = state
        .try_execute_immediately(
            &cert,
            ExecutionEnv::new().with_scheduling_source(SchedulingSource::MysticetiFastPath),
            &state.epoch_store_for_testing(),
        )
        .await
        .unwrap();

    // Verify execution succeeded
    if !effects.status().is_ok() {
        eprintln!("Execution failed: {:?}", effects.status());
        panic!("Transaction execution failed: {:?}", effects.status());
    }

    // Verify no gas was charged
    let gas_cost = effects.gas_cost_summary();
    assert_eq!(
        gas_cost.net_gas_usage(),
        0,
        "NativeTransfer should not charge gas even in fast path"
    );
}

#[tokio::test]
async fn test_native_transfer_full_amount() {
    let (sender, sender_key) = get_account_key_pair();
    let recipient = SuiAddress::random_for_testing_only();

    // Create a gas coin
    let coin_id = ObjectID::random();
    let coin_value = 1000;
    let gas_coin = GasCoin::new(coin_id, coin_value);
    let coin_object = Object::new_move(
        gas_coin.to_object(sui_types::base_types::SequenceNumber::from_u64(1)),
        sui_types::object::Owner::AddressOwner(sender),
        sui_types::base_types::TransactionDigest::ZERO,
    );
    let coin_ref = coin_object.compute_object_reference();

    let state = TestAuthorityBuilder::new()
        .with_protocol_config(protocol_config_v2())
        .with_starting_objects(&[coin_object])
        .build()
        .await;

    // Transfer the full amount
    let transfer_amount = coin_value;
    let tx_data =
        TransactionData::new_native_transfer(sender, coin_ref, recipient, transfer_amount);
    let signed_tx = to_sender_signed_transaction(tx_data, &sender_key);

    // Execute the transaction
    let (_cert, effects) = send_and_confirm_transaction(&state, None, signed_tx)
        .await
        .unwrap();

    // Verify execution succeeded
    if !effects.status().is_ok() {
        eprintln!("Execution failed: {:?}", effects.status());
        panic!("Transaction execution failed: {:?}", effects.status());
    }

    // Verify the source coin has zero balance
    let updated_coin = state.get_object(&coin_id).await.unwrap();
    let updated_gas_coin = GasCoin::try_from(&updated_coin).unwrap();
    assert_eq!(
        updated_gas_coin.value(),
        0,
        "Source coin should have zero balance after full transfer"
    );

    // Verify new coin has full amount
    let created_objects = effects.created();
    assert_eq!(created_objects.len(), 1, "Should create one new coin");
    let new_coin_id = created_objects[0].0.0;
    let new_coin = state.get_object(&new_coin_id).await.unwrap();
    let new_gas_coin = GasCoin::try_from(&new_coin).unwrap();
    assert_eq!(
        new_gas_coin.value(),
        transfer_amount,
        "New coin should have full transfer amount"
    );
}

#[tokio::test]
async fn test_native_transfer_multiple_transfers() {
    let (sender, sender_key) = get_account_key_pair();
    let recipient1 = SuiAddress::random_for_testing_only();
    let recipient2 = SuiAddress::random_for_testing_only();

    // Create a gas coin with enough for multiple transfers
    let coin_id = ObjectID::random();
    let coin_value = 2000;
    let gas_coin = GasCoin::new(coin_id, coin_value);
    let coin_object = Object::new_move(
        gas_coin.to_object(sui_types::base_types::SequenceNumber::from_u64(1)),
        sui_types::object::Owner::AddressOwner(sender),
        sui_types::base_types::TransactionDigest::ZERO,
    );
    let coin_ref = coin_object.compute_object_reference();

    let state = TestAuthorityBuilder::new()
        .with_protocol_config(protocol_config_v2())
        .with_starting_objects(&[coin_object])
        .build()
        .await;

    // First transfer
    let transfer_amount1 = 500;
    let tx_data1 =
        TransactionData::new_native_transfer(sender, coin_ref, recipient1, transfer_amount1);
    let signed_tx1 = to_sender_signed_transaction(tx_data1, &sender_key);
    let (_cert1, effects1) = send_and_confirm_transaction(&state, None, signed_tx1)
        .await
        .unwrap();
    if !effects1.status().is_ok() {
        eprintln!("First transfer execution failed: {:?}", effects1.status());
        panic!(
            "First transaction execution failed: {:?}",
            effects1.status()
        );
    }

    // Get updated coin reference
    let updated_coin1 = state.get_object(&coin_id).await.unwrap();
    let updated_coin_ref1 = updated_coin1.compute_object_reference();

    // Second transfer from the same coin
    let transfer_amount2 = 300;
    let tx_data2 = TransactionData::new_native_transfer(
        sender,
        updated_coin_ref1,
        recipient2,
        transfer_amount2,
    );
    let signed_tx2 = to_sender_signed_transaction(tx_data2, &sender_key);
    let (_cert2, effects2) = send_and_confirm_transaction(&state, None, signed_tx2)
        .await
        .unwrap();
    if !effects2.status().is_ok() {
        eprintln!("Second transfer execution failed: {:?}", effects2.status());
        panic!(
            "Second transaction execution failed: {:?}",
            effects2.status()
        );
    }

    // Verify final balance
    let final_coin = state.get_object(&coin_id).await.unwrap();
    let final_gas_coin = GasCoin::try_from(&final_coin).unwrap();
    assert_eq!(
        final_gas_coin.value(),
        coin_value - transfer_amount1 - transfer_amount2,
        "Final balance should be correct after multiple transfers"
    );

    // Verify both recipients received coins
    assert_eq!(effects1.created().len(), 1);
    assert_eq!(effects2.created().len(), 1);
}
