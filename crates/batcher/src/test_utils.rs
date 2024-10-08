use std::ops::Range;

use blockifier::blockifier::transaction_executor::VisitedSegmentsMapping;
use blockifier::bouncer::BouncerWeights;
use blockifier::state::cached_state::CommitmentStateDiff;
use indexmap::IndexMap;
use starknet_api::executable_transaction::Transaction;
use starknet_api::felt;
use starknet_api::test_utils::invoke::{executable_invoke_tx, InvokeTxArgs};
use starknet_api::transaction::fields::TransactionHash;

use crate::block_builder::BlockExecutionArtifacts;

pub fn test_txs(tx_hash_range: Range<usize>) -> Vec<Transaction> {
    tx_hash_range
        .map(|i| {
            Transaction::Invoke(executable_invoke_tx(InvokeTxArgs {
                tx_hash: TransactionHash(felt!(u128::try_from(i).unwrap())),
                ..Default::default()
            }))
        })
        .collect()
}

impl BlockExecutionArtifacts {
    pub fn create_for_testing() -> Self {
        Self {
            execution_infos: IndexMap::default(),
            commitment_state_diff: CommitmentStateDiff::default(),
            visited_segments_mapping: VisitedSegmentsMapping::default(),
            bouncer_weights: BouncerWeights::empty(),
        }
    }
}
