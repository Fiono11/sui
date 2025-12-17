// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::mock_account::Account;
use crate::tx_generator::TxGenerator;
use sui_types::base_types::SuiAddress;
use sui_types::transaction::{DEFAULT_VALIDATOR_GAS_PRICE, Transaction, TransactionData};

pub struct PaySuiNativeTxGenerator {
    num_recipients: u64,
    amount_per_recipient: u64,
}

impl PaySuiNativeTxGenerator {
    pub fn new(num_recipients: u64, amount_per_recipient: u64) -> Self {
        Self {
            num_recipients,
            amount_per_recipient,
        }
    }
}

impl TxGenerator for PaySuiNativeTxGenerator {
    fn generate_tx(&self, account: Account) -> Transaction {
        // For PaySuiNative, we need:
        // - coins: Vec<ObjectRef> - the coins to use for payment
        // - recipients: Vec<SuiAddress> - addresses to receive payment
        // - amounts: Vec<u64> - amounts each recipient receives
        // - gas_payment: ObjectRef - the first coin is used as gas payment
        // - gas_budget: u64 - gas budget for the transaction

        // Use the first coin as gas payment, and additional coins for payments
        // We need at least 1 coin (for gas), and ideally more for payments
        let coins: Vec<_> = account.gas_objects.iter().cloned().collect();

        // Generate recipient addresses
        // For benchmarking, we'll use the sender's address as recipients
        // (PaySuiNative allows sending to yourself)
        let recipients: Vec<SuiAddress> =
            (0..self.num_recipients).map(|_| account.sender).collect();
        let amounts: Vec<u64> = (0..self.num_recipients)
            .map(|_| self.amount_per_recipient)
            .collect();

        // Create the PaySuiNative transaction
        // The first coin is used as gas payment
        let gas_payment = coins[0];
        let gas_budget = 10_000_000; // Default gas budget

        let tx_data = TransactionData::new_pay_native(
            account.sender,
            coins,
            recipients,
            amounts,
            gas_payment,
            gas_budget,
            DEFAULT_VALIDATOR_GAS_PRICE,
        );

        Transaction::from_data_and_signer(tx_data, vec![account.keypair.as_ref()])
    }

    fn name(&self) -> &'static str {
        "PaySuiNative Transaction Generator"
    }
}
