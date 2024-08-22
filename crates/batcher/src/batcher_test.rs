use std::sync::Arc;

use assert_matches::assert_matches;
use async_trait::async_trait;
use futures::future::BoxFuture;
use futures::FutureExt;
use mockall::automock;
use mockall::predicate::eq;
use rstest::{fixture, rstest};
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::executable_transaction::Transaction;
use starknet_batcher_types::batcher_types::{
    BuildProposalInput,
    GetProposalContent,
    GetProposalContentInput,
    ProposalId,
    StartHeightInput,
};
use starknet_batcher_types::errors::BatcherError;

use crate::batcher::{Batcher, MockBatcherStorageReaderTrait};
use crate::config::BatcherConfig;
use crate::proposal_manager::{ProposalManagerResult, ProposalManagerTrait};
use crate::test_utils::test_txs;

const INITIAL_HEIGHT: BlockNumber = BlockNumber(3);
const STREAMING_CHUNK_SIZE: usize = 3;

#[fixture]
fn storage_reader() -> MockBatcherStorageReaderTrait {
    let mut storage = MockBatcherStorageReaderTrait::new();
    storage.expect_height().returning(|| Ok(INITIAL_HEIGHT));
    storage
}

#[fixture]
fn batcher_config() -> BatcherConfig {
    BatcherConfig { outstream_content_buffer_size: STREAMING_CHUNK_SIZE, ..Default::default() }
}

#[rstest]
#[tokio::test]
async fn get_stream_content(
    batcher_config: BatcherConfig,
    storage_reader: MockBatcherStorageReaderTrait,
) {
    starknet_mempool_infra::trace_util::configure_tracing();
    const PROPOSAL_ID: ProposalId = ProposalId(0);
    // Expecting 3 chunks of streamed txs.
    let expected_streamed_txs = test_txs(0..STREAMING_CHUNK_SIZE * 2 + 1);
    let txs_to_stream = expected_streamed_txs.clone();

    let mut proposal_manager = MockProposalManagerTraitWrapper::new();
    proposal_manager.expect_wrap_start_height().with(eq(INITIAL_HEIGHT)).return_once(|_| Ok(()));

    proposal_manager
        .expect_wrap_build_block_proposal()
        .withf(|proposal_id, _deadline, _tx_sender| *proposal_id == PROPOSAL_ID)
        .return_once(move |_proposal_id, _deadline, tx_sender| {
            simulate_build_block_proposal(tx_sender, txs_to_stream).boxed()
        });

    let mut batcher =
        Batcher::new(batcher_config, Arc::new(storage_reader), Box::new(proposal_manager));

    batcher.start_height(StartHeightInput { height: INITIAL_HEIGHT }).unwrap();
    batcher
        .build_proposal(BuildProposalInput {
            proposal_id: PROPOSAL_ID,
            deadline: chrono::Utc::now() + chrono::Duration::seconds(1),
            block_hash_10_blocks_ago: BlockHash::default(),
        })
        .await
        .unwrap();

    let expected_n_chunks = expected_streamed_txs.len().div_ceil(STREAMING_CHUNK_SIZE);
    let mut aggregated_streamed_txs = Vec::new();
    for _ in 0..expected_n_chunks {
        let content = batcher
            .get_proposal_content(GetProposalContentInput { proposal_id: PROPOSAL_ID })
            .await
            .unwrap()
            .content;
        let mut txs = assert_matches!(content, GetProposalContent::Txs(txs) => txs);
        assert!(txs.len() <= STREAMING_CHUNK_SIZE, "{} < {}", txs.len(), STREAMING_CHUNK_SIZE);
        aggregated_streamed_txs.append(&mut txs);
    }
    assert_eq!(aggregated_streamed_txs, expected_streamed_txs);

    // TODO: Test that we get `Finished` after all the txs are streamed once it is implemented.

    let exhausted =
        batcher.get_proposal_content(GetProposalContentInput { proposal_id: PROPOSAL_ID }).await;
    assert_matches!(exhausted, Err(BatcherError::StreamExhausted));
}

async fn simulate_build_block_proposal(
    tx_sender: tokio::sync::mpsc::UnboundedSender<Transaction>,
    txs: Vec<Transaction>,
) -> ProposalManagerResult<()> {
    for tx in txs {
        tx_sender.send(tx).unwrap();
    }
    drop(tx_sender);
    Ok(())
}

// A wrapper trait to allow mocking the ProposalManagerTrait in tests.
#[automock]
trait ProposalManagerTraitWrapper: Send + Sync {
    fn wrap_start_height(&mut self, height: BlockNumber) -> ProposalManagerResult<()>;

    fn wrap_build_block_proposal(
        &mut self,
        proposal_id: ProposalId,
        deadline: tokio::time::Instant,
        output_content_sender: tokio::sync::mpsc::UnboundedSender<Transaction>,
    ) -> BoxFuture<'_, ProposalManagerResult<()>>;
}

#[async_trait]
impl<T: ProposalManagerTraitWrapper> ProposalManagerTrait for T {
    fn start_height(&mut self, height: BlockNumber) -> ProposalManagerResult<()> {
        self.wrap_start_height(height)
    }

    async fn build_block_proposal(
        &mut self,
        proposal_id: ProposalId,
        deadline: tokio::time::Instant,
        output_content_sender: tokio::sync::mpsc::UnboundedSender<Transaction>,
    ) -> ProposalManagerResult<()> {
        self.wrap_build_block_proposal(proposal_id, deadline, output_content_sender).await
    }
}
