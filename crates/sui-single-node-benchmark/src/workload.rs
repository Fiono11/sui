// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::benchmark_context::BenchmarkContext;
use crate::command::WorkloadKind;
use crate::tx_generator::{MoveTxGenerator, PackagePublishTxGenerator, TxGenerator};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use sui_test_transaction_builder::PublishData;

#[derive(Clone)]
pub struct Workload {
    pub tx_count: u64,
    pub workload_kind: WorkloadKind,
}

impl Workload {
    pub fn new(tx_count: u64, workload_kind: WorkloadKind) -> Self {
        Self {
            tx_count,
            workload_kind,
        }
    }

    pub(crate) fn num_accounts(&self) -> u64 {
        self.tx_count
    }

    pub(crate) fn gas_object_num_per_account(&self) -> u64 {
        self.workload_kind.gas_object_num_per_account()
    }

    pub(crate) async fn create_tx_generator(
        &self,
        ctx: &mut BenchmarkContext,
    ) -> Arc<dyn TxGenerator> {
        match &self.workload_kind {
            WorkloadKind::PTB {
                num_transfers,
                use_native_transfer,
                num_dynamic_fields,
                computation,
                num_shared_objects,
                num_mints,
                nft_size,
                use_batch_mint,
                coin_ops_only,
            } => {
                let coin_ops_only = *coin_ops_only;

                let (move_package_id, root_objects, shared_objects) = if coin_ops_only {
                    (
                        sui_types::base_types::ObjectID::ZERO,
                        HashMap::new(),
                        Vec::new(),
                    )
                } else {
                    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
                    path.extend(["move_package"]);
                    let move_package = ctx.publish_package(PublishData::Source(path, false)).await;
                    let root_objects = ctx
                        .preparing_dynamic_fields(move_package.0, *num_dynamic_fields)
                        .await;
                    let shared_objects = ctx
                        .prepare_shared_objects(move_package.0, *num_shared_objects)
                        .await;
                    (move_package.0, root_objects, shared_objects)
                };

                let effective_use_native_transfer = if coin_ops_only {
                    true
                } else {
                    *use_native_transfer
                };
                let effective_computation = if coin_ops_only { 0 } else { *computation };
                let effective_num_mints = if coin_ops_only { 0 } else { *num_mints };
                let effective_use_batch_mint = if coin_ops_only {
                    false
                } else {
                    *use_batch_mint
                };

                Arc::new(MoveTxGenerator::new(
                    move_package_id,
                    *num_transfers,
                    effective_use_native_transfer,
                    effective_computation,
                    root_objects,
                    shared_objects,
                    effective_num_mints,
                    *nft_size,
                    effective_use_batch_mint,
                ))
            }
            WorkloadKind::Publish {
                manifest_file: manifest_path,
            } => Arc::new(PackagePublishTxGenerator::new(ctx, manifest_path.clone()).await),
        }
    }
}
