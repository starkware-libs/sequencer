use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use assert_matches::assert_matches;
use async_trait::async_trait;
use futures::future::BoxFuture;
use futures::FutureExt;
use mockall::automock;
use mockall::predicate::eq;
use rstest::{fixture, rstest};
use starknet_api::block::{BlockHashAndNumber, BlockNumber};
use starknet_api::core::{ContractAddress, Nonce, StateDiffCommitment};
use starknet_api::executable_transaction::Transaction;
use starknet_api::hash::PoseidonHash;
use starknet_api::state::ThinStateDiff;
use starknet_api::transaction::TransactionHash;
use starknet_api::{contract_address, felt, nonce};
use starknet_batcher_types::batcher_types::{
    BuildProposalInput,
    DecisionReachedInput,
    GetProposalContent,
    GetProposalContentInput,
    GetProposalContentResponse,
    ProposalCommitment,
    ProposalId,
    StartHeightInput,
};
use starknet_batcher_types::errors::BatcherError;
use starknet_mempool_types::communication::MockMempoolClient;
use starknet_mempool_types::mempool_types::CommitBlockArgs;

use crate::batcher::{Batcher, MockBatcherStorageReaderTrait, MockBatcherStorageWriterTrait};
use crate::config::BatcherConfig;
use crate::proposal_manager::{
    BuildProposalError,
    GetProposalResultError,
    ProposalManagerTrait,
    ProposalOutput,
    ProposalResult,
    StartHeightError,
};
use crate::test_utils::test_txs;
use crate::transaction_provider::ProposeTransactionProvider;

const INITIAL_HEIGHT: BlockNumber = BlockNumber(3);
const STREAMING_CHUNK_SIZE: usize = 3;

#[fixture]
fn storage_reader() -> MockBatcherStorageReaderTrait {
    let mut storage = MockBatcherStorageReaderTrait::new();
    storage.expect_height().returning(|| Ok(INITIAL_HEIGHT));
    storage
}

#[fixture]
fn storage_writer() -> MockBatcherStorageWriterTrait {
    MockBatcherStorageWriterTrait::new()
}

#[fixture]
fn batcher_config() -> BatcherConfig {
    BatcherConfig { outstream_content_buffer_size: STREAMING_CHUNK_SIZE, ..Default::default() }
}

#[fixture]
fn mempool_client() -> MockMempoolClient {
    MockMempoolClient::new()
}

#[rstest]
#[tokio::test]
async fn get_stream_content(
    batcher_config: BatcherConfig,
    storage_reader: MockBatcherStorageReaderTrait,
    storage_writer: MockBatcherStorageWriterTrait,
    mempool_client: MockMempoolClient,
) {
    const PROPOSAL_ID: ProposalId = ProposalId(0);
    // Expecting 3 chunks of streamed txs.
    let expected_streamed_txs = test_txs(0..STREAMING_CHUNK_SIZE * 2 + 1);
    let txs_to_stream = expected_streamed_txs.clone();
    let expected_proposal_commitment = ProposalCommitment {
        state_diff_commitment: StateDiffCommitment(PoseidonHash(felt!(u128::try_from(7).unwrap()))),
    };

    let mut proposal_manager = MockProposalManagerTraitWrapper::new();
    proposal_manager.expect_wrap_start_height().return_once(|_| async { Ok(()) }.boxed());
    proposal_manager.expect_wrap_build_block_proposal().return_once(
        move |_proposal_id, _block_hash, _deadline, tx_sender, _tx_provider| {
            simulate_build_block_proposal(tx_sender, txs_to_stream).boxed()
        },
    );
    proposal_manager
        .expect_wrap_executed_proposal_commitment()
        .return_once(move |_| async move { Ok(expected_proposal_commitment) }.boxed());

    let mut batcher = Batcher::new(
        batcher_config,
        Arc::new(storage_reader),
        Box::new(storage_writer),
        Arc::new(mempool_client),
        Box::new(proposal_manager),
    );

    batcher.start_height(StartHeightInput { height: INITIAL_HEIGHT }).await.unwrap();
    batcher
        .build_proposal(BuildProposalInput {
            proposal_id: PROPOSAL_ID,
            retrospective_block_hash: None,
            deadline: chrono::Utc::now() + chrono::Duration::seconds(1),
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

    let commitment = batcher
        .get_proposal_content(GetProposalContentInput { proposal_id: PROPOSAL_ID })
        .await
        .unwrap();
    assert_matches!(
        commitment,
        GetProposalContentResponse {
            content: GetProposalContent::Finished(proposal_commitment)
        } if proposal_commitment == expected_proposal_commitment
    );

    let exhausted =
        batcher.get_proposal_content(GetProposalContentInput { proposal_id: PROPOSAL_ID }).await;
    assert_matches!(exhausted, Err(BatcherError::ProposalNotFound { .. }));
}

#[rstest]
#[tokio::test]
async fn decision_reached(
    batcher_config: BatcherConfig,
    storage_reader: MockBatcherStorageReaderTrait,
    mut storage_writer: MockBatcherStorageWriterTrait,
    mut mempool_client: MockMempoolClient,
) {
    const PROPOSAL_ID: ProposalId = ProposalId(0);
    let expected_state_diff = ThinStateDiff::default();
    let state_diff_clone = expected_state_diff.clone();
    let expected_proposal_commitment = ProposalCommitment::default();
    let tx_hashes = test_tx_hashes(0..5);
    let tx_hashes_clone = tx_hashes.clone();
    let address_to_nonce = test_contract_nonces(0..3);
    let nonces_clone = address_to_nonce.clone();

    let mut proposal_manager = MockProposalManagerTraitWrapper::new();
    proposal_manager.expect_wrap_take_proposal_result().with(eq(PROPOSAL_ID)).return_once(
        move |_| {
            async move {
                Ok(ProposalOutput {
                    state_diff: state_diff_clone,
                    commitment: expected_proposal_commitment,
                    tx_hashes: tx_hashes_clone,
                    nonces: nonces_clone,
                })
            }
            .boxed()
        },
    );
    mempool_client
        .expect_commit_block()
        .with(eq(CommitBlockArgs { address_to_nonce, tx_hashes }))
        .returning(|_| Ok(()));

    storage_writer
        .expect_commit_proposal()
        .with(eq(INITIAL_HEIGHT), eq(expected_state_diff))
        .returning(|_, _| Ok(()));

    let mut batcher = Batcher::new(
        batcher_config,
        Arc::new(storage_reader),
        Box::new(storage_writer),
        Arc::new(mempool_client),
        Box::new(proposal_manager),
    );
    batcher.decision_reached(DecisionReachedInput { proposal_id: ProposalId(0) }).await.unwrap();
}

#[rstest]
#[tokio::test]
async fn decision_reached_no_executed_proposal(
    batcher_config: BatcherConfig,
    storage_reader: MockBatcherStorageReaderTrait,
    storage_writer: MockBatcherStorageWriterTrait,
    mempool_client: MockMempoolClient,
) {
    const PROPOSAL_ID: ProposalId = ProposalId(0);
    let expected_error = BatcherError::ExecutedProposalNotFound { proposal_id: PROPOSAL_ID };

    let mut proposal_manager = MockProposalManagerTraitWrapper::new();
    proposal_manager.expect_wrap_take_proposal_result().with(eq(PROPOSAL_ID)).return_once(
        |proposal_id| {
            async move { Err(GetProposalResultError::ProposalDoesNotExist { proposal_id }) }.boxed()
        },
    );

    let mut batcher = Batcher::new(
        batcher_config,
        Arc::new(storage_reader),
        Box::new(storage_writer),
        Arc::new(mempool_client),
        Box::new(proposal_manager),
    );
    let decision_reached_result =
        batcher.decision_reached(DecisionReachedInput { proposal_id: PROPOSAL_ID }).await;
    assert_eq!(decision_reached_result, Err(expected_error));
}

async fn simulate_build_block_proposal(
    tx_sender: tokio::sync::mpsc::UnboundedSender<Transaction>,
    txs: Vec<Transaction>,
) -> Result<(), BuildProposalError> {
    tokio::spawn(async move {
        for tx in txs {
            tx_sender.send(tx).unwrap();
        }
    });
    Ok(())
}

// A wrapper trait to allow mocking the ProposalManagerTrait in tests.
#[automock]
trait ProposalManagerTraitWrapper: Send + Sync {
    fn wrap_start_height(
        &mut self,
        height: BlockNumber,
    ) -> BoxFuture<'_, Result<(), StartHeightError>>;

    fn wrap_build_block_proposal(
        &mut self,
        proposal_id: ProposalId,
        retrospective_block_hash: Option<BlockHashAndNumber>,
        deadline: tokio::time::Instant,
        output_content_sender: tokio::sync::mpsc::UnboundedSender<Transaction>,
        tx_provider: ProposeTransactionProvider,
    ) -> BoxFuture<'_, Result<(), BuildProposalError>>;

    fn wrap_take_proposal_result(
        &mut self,
        proposal_id: ProposalId,
    ) -> BoxFuture<'_, ProposalResult<ProposalOutput>>;

    fn wrap_executed_proposal_commitment(
        &self,
        proposal_id: ProposalId,
    ) -> BoxFuture<'_, ProposalResult<ProposalCommitment>>;
}

#[async_trait]
impl<T: ProposalManagerTraitWrapper> ProposalManagerTrait for T {
    async fn start_height(&mut self, height: BlockNumber) -> Result<(), StartHeightError> {
        self.wrap_start_height(height).await
    }

    async fn build_block_proposal(
        &mut self,
        proposal_id: ProposalId,
        retrospective_block_hash: Option<BlockHashAndNumber>,
        deadline: tokio::time::Instant,
        output_content_sender: tokio::sync::mpsc::UnboundedSender<Transaction>,
        tx_provider: ProposeTransactionProvider,
    ) -> Result<(), BuildProposalError> {
        self.wrap_build_block_proposal(
            proposal_id,
            retrospective_block_hash,
            deadline,
            output_content_sender,
            tx_provider,
        )
        .await
    }

    async fn take_proposal_result(
        &mut self,
        proposal_id: ProposalId,
    ) -> ProposalResult<ProposalOutput> {
        self.wrap_take_proposal_result(proposal_id).await
    }

    async fn get_executed_proposal_commitment(
        &self,
        proposal_id: ProposalId,
    ) -> ProposalResult<ProposalCommitment> {
        self.wrap_executed_proposal_commitment(proposal_id).await
    }
}

fn test_tx_hashes(range: std::ops::Range<u128>) -> HashSet<TransactionHash> {
    range.map(|i| TransactionHash(felt!(i))).collect()
}

fn test_contract_nonces(range: std::ops::Range<u128>) -> HashMap<ContractAddress, Nonce> {
    HashMap::from_iter(range.map(|i| (contract_address!(i), nonce!(i))))
}
