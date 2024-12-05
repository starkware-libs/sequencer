use std::mem;
use std::ops::Range;

use async_trait::async_trait;
use blockifier::blockifier::transaction_executor::VisitedSegmentsMapping;
use blockifier::bouncer::BouncerWeights;
use blockifier::state::cached_state::CommitmentStateDiff;
use indexmap::IndexMap;
use starknet_api::executable_transaction::Transaction;
use starknet_api::test_utils::invoke::{executable_invoke_tx, InvokeTxArgs};
use starknet_api::{class_hash, contract_address, nonce, tx_hash};
use tokio::sync::mpsc::UnboundedSender;

use crate::block_builder::{BlockBuilderResult, BlockBuilderTrait, BlockExecutionArtifacts};
use crate::transaction_provider::{NextTxs, TransactionProvider};

// A fake block builder for validate flow, that fetches transactions from the transaction provider
// until it is exhausted.
// This ensures the block builder (and specifically the tx_provider) is not dropped before all
// transactions are processed. Otherwise, the batcher would fail during tests when attempting to
// send transactions to it.
pub(crate) struct FakeValidateBlockBuilder {
    pub tx_provider: Box<dyn TransactionProvider>,
    pub build_block_result: BlockBuilderResult<BlockExecutionArtifacts>,
}

#[async_trait]
impl BlockBuilderTrait for FakeValidateBlockBuilder {
    async fn build_block(&mut self) -> BlockBuilderResult<BlockExecutionArtifacts> {
        if self.build_block_result.is_ok() {
            while self.tx_provider.get_txs(1).await.is_ok_and(|v| v != NextTxs::End) {
                tokio::task::yield_now().await;
            }
        }
        // build_block should be called only once, so we can safely take the result.
        // TODO: use an Option instead of mem::replace.
        mem::replace(
            &mut self.build_block_result,
            Ok(BlockExecutionArtifacts::create_for_testing()),
        )
    }
}

// A fake block builder for propose flow, that sends the given transactions to the output content
// sender.
pub(crate) struct FakeProposeBlockBuilder {
    pub output_content_sender: UnboundedSender<Transaction>,
    pub output_txs: Vec<Transaction>,
    pub build_block_result: BlockBuilderResult<BlockExecutionArtifacts>,
}

#[async_trait]
impl BlockBuilderTrait for FakeProposeBlockBuilder {
    async fn build_block(&mut self) -> BlockBuilderResult<BlockExecutionArtifacts> {
        for tx in &self.output_txs {
            self.output_content_sender.send(tx.clone()).unwrap();
        }
        // build_block should be called only once, so we can safely take the result.
        mem::replace(
            &mut self.build_block_result,
            Ok(BlockExecutionArtifacts::create_for_testing()),
        )
    }
}

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
            visited_segments_mapping: VisitedSegmentsMapping::default(),
            bouncer_weights: BouncerWeights::empty(),
        }
    }
}
