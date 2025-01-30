use std::ops::Range;

use blockifier::bouncer::BouncerWeights;
use blockifier::state::cached_state::CommitmentStateDiff;
use indexmap::IndexMap;
use starknet_api::executable_transaction::Transaction;
use starknet_api::execution_resources::GasAmount;
use starknet_api::test_utils::invoke::{executable_invoke_tx, InvokeTxArgs};
use starknet_api::{class_hash, contract_address, nonce, tx_hash};

use crate::block_builder::BlockExecutionArtifacts;

pub fn test_txs(tx_hash_range: Range<usize>) -> Vec<Transaction> {
    tx_hash_range
        .map(|i| {
            Transaction::Account(executable_invoke_tx(InvokeTxArgs {
                tx_hash: tx_hash!(i),
                ..Default::default()
            }))
        })
        .collect()
}

impl BlockExecutionArtifacts {
    pub fn create_for_testing() -> Self {
        // Use a non-empty commitment_state_diff to make the tests more realistic.
        Self {
            execution_infos: IndexMap::default(),
            commitment_state_diff: CommitmentStateDiff {
                address_to_class_hash: IndexMap::from_iter([(
                    contract_address!("0x7"),
                    class_hash!("0x11111111"),
                )]),
                storage_updates: IndexMap::new(),
                class_hash_to_compiled_class_hash: IndexMap::new(),
                address_to_nonce: IndexMap::from_iter([(contract_address!("0x7"), nonce!(1_u64))]),
            },
            bouncer_weights: BouncerWeights::empty(),
            l2_gas_used: GasAmount::default(),
        }
    }
}
