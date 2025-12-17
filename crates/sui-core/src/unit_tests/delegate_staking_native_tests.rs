// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_test_utils::send_and_confirm_transaction_with_execution_error;
use crate::authority::test_authority_builder::TestAuthorityBuilder;
use bcs;
use move_core_types::identifier::Identifier;
use sui_types::base_types::{ObjectDigest, ObjectID, SequenceNumber};
use sui_types::crypto::AccountKeyPair;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::execution_status::ExecutionStatus;
use sui_types::governance::StakedSui;
use sui_types::object::Object;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::TransactionData;
use sui_types::utils::to_sender_signed_transaction;
use sui_types::{
    SUI_SYSTEM_PACKAGE_ID, crypto::get_key_pair, sui_system_state::SuiSystemStateTrait,
};

const MIST_PER_SUI: u64 = 1_000_000_000;

#[tokio::test]
// Scenario:
// 1. Stake 60 SUI to VALIDATOR_ADDR_1 using native DelegateStakingNative
// 2. Split the stake into 20 and 40 using Move
// 3. Join the 20 and 40 back together using Move
// 4. Check that the stake is 60 again
async fn test_split_join_staked_sui_native() -> anyhow::Result<()> {
    let (staker, staker_key): (_, AccountKeyPair) = get_key_pair();

    // Create test authority with default genesis (includes validators)
    let authority_state = TestAuthorityBuilder::new().build().await;
    let rgp = authority_state.reference_gas_price_for_testing().unwrap();

    // Create coins: one for staking, one for gas
    // For DelegateStakingNative (system transaction), the gas payment must be in the coins list
    // We'll use the first coin as both staking and gas (like PaySuiNative)
    let stake_coin_obj = Object::with_id_owner_gas_for_testing(
        ObjectID::random(),
        staker,
        100 * MIST_PER_SUI, // 100 SUI for staking
    );
    // Create a separate gas coin for subsequent transactions
    let gas_coin_obj = Object::with_id_owner_gas_for_testing(
        ObjectID::random(),
        staker,
        10 * MIST_PER_SUI, // 10 SUI for gas in subsequent transactions
    );
    authority_state
        .insert_genesis_object(stake_coin_obj.clone())
        .await;
    authority_state
        .insert_genesis_object(gas_coin_obj.clone())
        .await;
    let stake_coin_ref = stake_coin_obj.compute_object_reference();

    // Get a validator address from the system state summary
    let system_state = authority_state.get_sui_system_state_object_for_testing()?;
    let system_state_summary = SuiSystemStateTrait::into_sui_system_state_summary(system_state);
    let validator_addr = system_state_summary
        .active_validators
        .first()
        .ok_or_else(|| anyhow::anyhow!("No validators found"))?
        .sui_address;

    // Step 1: Stake 60 SUI using native DelegateStakingNative
    // DelegateStakingNative is a system transaction and doesn't require a gas object
    let stake_amount = 60 * MIST_PER_SUI;
    let data = TransactionData::new_delegate_staking_native(
        staker,
        vec![stake_coin_ref], // Coin for staking
        validator_addr,
        Some(stake_amount),
        // Dummy gas payment - not used for system transactions
        (ObjectID::ZERO, SequenceNumber::default(), ObjectDigest::MIN),
        0, // Zero gas budget - system transaction style
        rgp,
    );

    let tx = to_sender_signed_transaction(data, &staker_key);
    // DelegateStakingNative uses the system state shared object, so we need with_shared=true
    // Use fake_consensus=false to assign versions directly without going through consensus
    let (_, effects, _) = send_and_confirm_transaction_with_execution_error(
        &authority_state,
        None,
        tx,
        true,  // with_shared
        false, // fake_consensus - assign versions directly
    )
    .await?;
    assert_eq!(*effects.status(), ExecutionStatus::Success);

    // Find the StakedSui object - check both created() and mutated() lists
    // The object might have been created and then mutated in the same transaction
    let staked_sui_id = effects
        .created()
        .iter()
        .find(|(_, owner)| owner.get_owner_address().unwrap() == staker)
        .map(|(id, _)| id.0)
        .or_else(|| {
            effects
                .mutated()
                .iter()
                .find(|(_, owner)| owner.get_owner_address().unwrap() == staker)
                .map(|(id, _)| id.0)
        })
        .ok_or_else(|| anyhow::anyhow!("StakedSui object not found"))?;

    // Get the current object from the database - this will have the latest version
    // Re-fetch right before building the transaction to ensure we have the absolute latest version
    let staked_sui_obj = authority_state
        .get_object(&staked_sui_id)
        .await
        .ok_or_else(|| anyhow::anyhow!("StakedSui object not found after creation"))?;
    let staked_sui = StakedSui::try_from(&staked_sui_obj)?;
    assert_eq!(staked_sui.principal(), stake_amount);

    // Re-fetch one more time right before building the transaction to ensure we have the latest version
    // The object may have been mutated after creation, so we need the current version from the database
    let staked_sui_obj_final = authority_state
        .get_object(&staked_sui_id)
        .await
        .ok_or_else(|| anyhow::anyhow!("StakedSui object not found"))?;
    // Use the object reference computed from the database object, which should have the latest version
    let staked_sui_ref = staked_sui_obj_final.compute_object_reference();

    // Use the separate gas coin for subsequent transactions
    // Get the current reference (in case it was mutated)
    let gas_coin_obj_current = authority_state
        .get_object(&gas_coin_obj.id())
        .await
        .ok_or_else(|| anyhow::anyhow!("Gas coin not found"))?;
    let gas_coin = gas_coin_obj_current.compute_object_reference();

    // Step 2: Split the stake into 20 and 40 using Move
    let split_amount = 20 * MIST_PER_SUI;
    let mut builder = ProgrammableTransactionBuilder::new();
    builder.move_call(
        SUI_SYSTEM_PACKAGE_ID,
        Identifier::new("staking_pool").unwrap(),
        Identifier::new("split_staked_sui").unwrap(),
        vec![],
        vec![
            sui_types::transaction::CallArg::Object(
                sui_types::transaction::ObjectArg::ImmOrOwnedObject(staked_sui_ref),
            ),
            sui_types::transaction::CallArg::Pure(bcs::to_bytes(&split_amount)?),
        ],
    )?;
    let pt = builder.finish();

    let data = TransactionData::new_programmable(
        staker,
        vec![gas_coin], // Only gas coins should be in gas_payment
        pt,
        10_000_000, // Gas budget
        rgp,
    );
    let tx = to_sender_signed_transaction(data, &staker_key);
    let (_, effects, _) = send_and_confirm_transaction_with_execution_error(
        &authority_state,
        None,
        tx,
        false, // with_shared - split doesn't use shared objects
        false, // fake_consensus
    )
    .await?;
    assert_eq!(*effects.status(), ExecutionStatus::Success);

    // Get both StakedSui objects (one created, one mutated)
    let mut all_staked_sui = vec![];

    // Get the newly created StakedSui from split
    for (id, owner) in effects.created() {
        if owner.get_owner_address().unwrap() == staker {
            if let Some(obj) = authority_state.get_object(&id.0).await {
                if let Some(move_obj) = obj.data.try_as_move() {
                    if move_obj.type_().is_staked_sui() {
                        all_staked_sui.push((id.0, obj));
                    }
                }
            }
        }
    }

    // Get the mutated StakedSui (the original one with remaining amount)
    for (id, _) in effects.mutated() {
        if let Some(obj) = authority_state.get_object(&id.0).await {
            if let Some(move_obj) = obj.data.try_as_move() {
                if move_obj.type_().is_staked_sui() {
                    all_staked_sui.push((id.0, obj));
                }
            }
        }
    }

    assert_eq!(
        all_staked_sui.len(),
        2,
        "Should have 2 StakedSui objects after split"
    );

    // Verify amounts: one should be 20 SUI, the other 40 SUI
    let amounts: Vec<u64> = all_staked_sui
        .iter()
        .map(|(_, obj)| StakedSui::try_from(obj).unwrap().principal())
        .collect();
    assert!(amounts.contains(&(20 * MIST_PER_SUI)));
    assert!(amounts.contains(&(40 * MIST_PER_SUI)));

    // Step 3: Join the stakes back together using Move
    // Re-fetch the StakedSui objects right before building the transaction to ensure we have the latest versions
    let staked_sui_0_id = all_staked_sui[0].0;
    let staked_sui_1_id = all_staked_sui[1].0;
    let staked_sui_0_obj = authority_state
        .get_object(&staked_sui_0_id)
        .await
        .ok_or_else(|| anyhow::anyhow!("StakedSui object 0 not found"))?;
    let staked_sui_1_obj = authority_state
        .get_object(&staked_sui_1_id)
        .await
        .ok_or_else(|| anyhow::anyhow!("StakedSui object 1 not found"))?;

    let mut builder = ProgrammableTransactionBuilder::new();
    builder.move_call(
        SUI_SYSTEM_PACKAGE_ID,
        Identifier::new("staking_pool").unwrap(),
        Identifier::new("join_staked_sui").unwrap(),
        vec![],
        vec![
            sui_types::transaction::CallArg::Object(
                sui_types::transaction::ObjectArg::ImmOrOwnedObject(
                    staked_sui_0_obj.compute_object_reference(),
                ),
            ),
            sui_types::transaction::CallArg::Object(
                sui_types::transaction::ObjectArg::ImmOrOwnedObject(
                    staked_sui_1_obj.compute_object_reference(),
                ),
            ),
        ],
    )?;
    let pt = builder.finish();

    // Re-fetch the gas coin to ensure we have the latest version after the split transaction
    let gas_coin_obj_after_split = authority_state
        .get_object(&gas_coin_obj.id())
        .await
        .ok_or_else(|| anyhow::anyhow!("Gas coin not found"))?;
    let gas_coin_after_split = gas_coin_obj_after_split.compute_object_reference();

    let data = TransactionData::new_programmable(
        staker,
        vec![gas_coin_after_split], // Only gas coins should be in gas_payment
        pt,
        10_000_000,
        rgp,
    );
    let tx = to_sender_signed_transaction(data, &staker_key);
    let (_, effects, _) = send_and_confirm_transaction_with_execution_error(
        &authority_state,
        None,
        tx,
        false, // with_shared - join doesn't use shared objects
        false, // fake_consensus
    )
    .await?;
    assert_eq!(*effects.status(), ExecutionStatus::Success);

    // Step 4: Verify the final stake is 60 SUI
    // One StakedSui should be deleted, one should remain with 60 SUI
    // We know which objects were joined, so check if one is deleted and one is mutated
    let deleted_ids: std::collections::HashSet<_> = effects
        .deleted()
        .iter()
        .map(|(id, _, _)| *id)
        .chain(
            effects
                .unwrapped_then_deleted()
                .iter()
                .map(|(id, _, _)| *id),
        )
        .collect();

    let mutated_ids: std::collections::HashSet<_> =
        effects.mutated().iter().map(|(id, _)| id.0).collect();

    // One of the StakedSui objects should be deleted, one should be mutated
    let deleted_count = [staked_sui_0_id, staked_sui_1_id]
        .iter()
        .filter(|id| deleted_ids.contains(id))
        .count();
    let mutated_count = [staked_sui_0_id, staked_sui_1_id]
        .iter()
        .filter(|id| mutated_ids.contains(id))
        .count();

    assert_eq!(deleted_count, 1, "Should have 1 deleted StakedSui");
    assert_eq!(mutated_count, 1, "Should have 1 mutated StakedSui");

    // Find the mutated StakedSui (the one that wasn't deleted)
    let final_staked_sui_id = if mutated_ids.contains(&staked_sui_0_id) {
        staked_sui_0_id
    } else {
        staked_sui_1_id
    };
    let final_staked_sui_obj = authority_state
        .get_object(&final_staked_sui_id)
        .await
        .ok_or_else(|| anyhow::anyhow!("Final StakedSui object not found"))?;
    let final_staked_sui = StakedSui::try_from(&final_staked_sui_obj)?;
    assert_eq!(final_staked_sui.principal(), 60 * MIST_PER_SUI);

    Ok(())
}
