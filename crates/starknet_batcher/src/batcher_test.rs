use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use assert_matches::assert_matches;
use async_trait::async_trait;
use blockifier::blockifier::transaction_executor::TransactionExecutorError;
use chrono::Utc;
use futures::future::BoxFuture;
use futures::FutureExt;
use mockall::automock;
use mockall::predicate::{always, eq};
use rstest::{fixture, rstest};
use starknet_api::block::{BlockHashAndNumber, BlockNumber};
use starknet_api::core::{ContractAddress, Nonce, StateDiffCommitment};
use starknet_api::executable_transaction::Transaction;
use starknet_api::hash::PoseidonHash;
use starknet_api::state::ThinStateDiff;
use starknet_api::transaction::TransactionHash;
use starknet_api::{contract_address, felt, nonce};
use starknet_batcher_types::batcher_types::{
    DecisionReachedInput,
    GetProposalContent,
    GetProposalContentInput,
    GetProposalContentResponse,
    ProposalCommitment,
    ProposalId,
    ProposalStatus,
    ProposeBlockInput,
    SendProposalContent,
    SendProposalContentInput,
    SendProposalContentResponse,
    StartHeightInput,
    ValidateBlockInput,
};
use starknet_batcher_types::errors::BatcherError;
use starknet_mempool_types::communication::MockMempoolClient;
use starknet_mempool_types::mempool_types::CommitBlockArgs;

use crate::batcher::{Batcher, MockBatcherStorageReaderTrait, MockBatcherStorageWriterTrait};
use crate::block_builder::BlockBuilderError;
use crate::config::BatcherConfig;
use crate::proposal_manager::{
    GenerateProposalError,
    GetProposalResultError,
    InternalProposalStatus,
    ProposalManagerTrait,
    ProposalOutput,
    ProposalResult,
    StartHeightError,
};
use crate::test_utils::test_txs;
use crate::transaction_provider::{ProposeTransactionProvider, ValidateTransactionProvider};

const INITIAL_HEIGHT: BlockNumber = BlockNumber(3);
const STREAMING_CHUNK_SIZE: usize = 3;
const BLOCK_GENERATION_TIMEOUT: tokio::time::Duration = tokio::time::Duration::from_secs(1);
const PROPOSAL_ID: ProposalId = ProposalId(0);

fn proposal_commitment() -> ProposalCommitment {
    ProposalCommitment {
        state_diff_commitment: StateDiffCommitment(PoseidonHash(felt!(u128::try_from(7).unwrap()))),
    }
}

fn deadline() -> chrono::DateTime<Utc> {
    chrono::Utc::now() + BLOCK_GENERATION_TIMEOUT
}

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

fn batcher(proposal_manager: MockProposalManagerTraitWrapper) -> Batcher {
    Batcher::new(
        batcher_config(),
        Arc::new(storage_reader()),
        Box::new(storage_writer()),
        Arc::new(mempool_client()),
        Box::new(proposal_manager),
    )
}

fn mock_proposal_manager_common_expectations(
    proposal_manager: &mut MockProposalManagerTraitWrapper,
) {
    proposal_manager
        .expect_wrap_start_height()
        .times(1)
        .with(eq(INITIAL_HEIGHT))
        .return_once(|_| async { Ok(()) }.boxed());
    proposal_manager
        .expect_wrap_await_proposal_commitment()
        .times(1)
        .with(eq(PROPOSAL_ID))
        .return_once(move |_| { async move { Ok(proposal_commitment()) } }.boxed());
}

fn mock_proposal_manager_validate_flow() -> MockProposalManagerTraitWrapper {
    let mut proposal_manager = MockProposalManagerTraitWrapper::new();
    mock_proposal_manager_common_expectations(&mut proposal_manager);
    proposal_manager
        .expect_wrap_validate_block()
        .times(1)
        .with(eq(PROPOSAL_ID), eq(None), always(), always())
        .return_once(|_, _, _, tx_provider| {
            {
                async move {
                    // Spawn a task to keep tx_provider alive until the transactions sender is
                    // dropped. Without this, the provider would be dropped,
                    // causing the batcher to fail when sending transactions to
                    // it during the test.
                    tokio::spawn(async move {
                        while !tx_provider.tx_receiver.is_closed() {
                            tokio::task::yield_now().await;
                        }
                    });
                    Ok(())
                }
            }
            .boxed()
        });
    proposal_manager
        .expect_wrap_get_proposal_status()
        .times(1)
        .with(eq(PROPOSAL_ID))
        .returning(move |_| async move { InternalProposalStatus::Processing }.boxed());
    proposal_manager
}

#[rstest]
#[tokio::test]
async fn validate_block_full_flow() {
    let proposal_manager = mock_proposal_manager_validate_flow();
    let mut batcher = batcher(proposal_manager);

    // TODO(Yael 14/11/2024): The test will pass without calling start height (if we delete the mock
    // expectation). Leaving this here for future compatibility with the upcoming
    // batcher-proposal_manager unification.
    batcher.start_height(StartHeightInput { height: INITIAL_HEIGHT }).await.unwrap();

    let validate_block_input = ValidateBlockInput {
        proposal_id: PROPOSAL_ID,
        deadline: deadline(),
        retrospective_block_hash: None,
    };
    batcher.validate_block(validate_block_input).await.unwrap();

    let send_proposal_input_txs = SendProposalContentInput {
        proposal_id: PROPOSAL_ID,
        content: SendProposalContent::Txs(test_txs(0..1)),
    };
    let send_txs_result = batcher.send_proposal_content(send_proposal_input_txs).await.unwrap();
    assert_eq!(
        send_txs_result,
        SendProposalContentResponse { response: ProposalStatus::Processing }
    );

    let send_proposal_input_finish =
        SendProposalContentInput { proposal_id: PROPOSAL_ID, content: SendProposalContent::Finish };
    let send_finish_result =
        batcher.send_proposal_content(send_proposal_input_finish).await.unwrap();
    assert_eq!(
        send_finish_result,
        SendProposalContentResponse { response: ProposalStatus::Finished(proposal_commitment()) }
    );
}

#[rstest]
#[tokio::test]
async fn send_content_after_proposal_already_finished() {
    let mut proposal_manager = MockProposalManagerTraitWrapper::new();
    proposal_manager
        .expect_wrap_get_proposal_status()
        .with(eq(PROPOSAL_ID))
        .times(1)
        .returning(|_| async move { InternalProposalStatus::Finished }.boxed());

    let mut batcher = batcher(proposal_manager);

    // Send transactions after the proposal has finished.
    let send_proposal_input_txs = SendProposalContentInput {
        proposal_id: PROPOSAL_ID,
        content: SendProposalContent::Txs(test_txs(0..1)),
    };
    let result = batcher.send_proposal_content(send_proposal_input_txs).await;
    assert_eq!(result, Err(BatcherError::ProposalAlreadyFinished { proposal_id: PROPOSAL_ID }));
}

#[rstest]
#[tokio::test]
async fn send_content_to_unknown_proposal() {
    let mut proposal_manager = MockProposalManagerTraitWrapper::new();
    proposal_manager
        .expect_wrap_get_proposal_status()
        .times(1)
        .with(eq(PROPOSAL_ID))
        .return_once(move |_| async move { InternalProposalStatus::NotFound }.boxed());

    let mut batcher = batcher(proposal_manager);

    // Send transactions to an unknown proposal.
    let send_proposal_input_txs = SendProposalContentInput {
        proposal_id: PROPOSAL_ID,
        content: SendProposalContent::Txs(test_txs(0..1)),
    };
    let result = batcher.send_proposal_content(send_proposal_input_txs).await;
    assert_eq!(result, Err(BatcherError::ProposalNotFound { proposal_id: PROPOSAL_ID }));

    // Send finish to an unknown proposal.
    let send_proposal_input_txs =
        SendProposalContentInput { proposal_id: PROPOSAL_ID, content: SendProposalContent::Finish };
    let result = batcher.send_proposal_content(send_proposal_input_txs).await;
    assert_eq!(result, Err(BatcherError::ProposalNotFound { proposal_id: PROPOSAL_ID }));
}

#[rstest]
#[tokio::test]
async fn send_txs_to_an_invalid_proposal() {
    let mut proposal_manager = MockProposalManagerTraitWrapper::new();
    proposal_manager
        .expect_wrap_get_proposal_status()
        .times(1)
        .with(eq(PROPOSAL_ID))
        .return_once(move |_| async move { InternalProposalStatus::Failed }.boxed());

    let mut batcher = batcher(proposal_manager);

    let send_proposal_input_txs = SendProposalContentInput {
        proposal_id: PROPOSAL_ID,
        content: SendProposalContent::Txs(test_txs(0..1)),
    };
    let result = batcher.send_proposal_content(send_proposal_input_txs).await.unwrap();
    assert_eq!(result, SendProposalContentResponse { response: ProposalStatus::InvalidProposal });
}

#[rstest]
#[tokio::test]
async fn send_finish_to_an_invalid_proposal() {
    let mut proposal_manager = MockProposalManagerTraitWrapper::new();
    proposal_manager
        .expect_wrap_validate_block()
        .times(1)
        .with(eq(PROPOSAL_ID), eq(None), always(), always())
        .return_once(|_, _, _, _| { async move { Ok(()) } }.boxed());

    let proposal_error = GetProposalResultError::BlockBuilderError(Arc::new(
        BlockBuilderError::FailOnError(TransactionExecutorError::BlockFull),
    ));
    proposal_manager
        .expect_wrap_await_proposal_commitment()
        .times(1)
        .with(eq(PROPOSAL_ID))
        .return_once(move |_| { async move { Err(proposal_error) } }.boxed());

    let mut batcher = batcher(proposal_manager);

    let validate_block_input = ValidateBlockInput {
        proposal_id: PROPOSAL_ID,
        deadline: deadline(),
        retrospective_block_hash: None,
    };
    batcher.validate_block(validate_block_input).await.unwrap();

    let send_proposal_input_txs =
        SendProposalContentInput { proposal_id: PROPOSAL_ID, content: SendProposalContent::Finish };
    let result = batcher.send_proposal_content(send_proposal_input_txs).await.unwrap();
    assert_eq!(result, SendProposalContentResponse { response: ProposalStatus::InvalidProposal });
}

#[rstest]
#[tokio::test]
async fn propose_block_full_flow() {
    // Expecting 3 chunks of streamed txs.
    let expected_streamed_txs = test_txs(0..STREAMING_CHUNK_SIZE * 2 + 1);
    let txs_to_stream = expected_streamed_txs.clone();

    let mut proposal_manager = MockProposalManagerTraitWrapper::new();
    mock_proposal_manager_common_expectations(&mut proposal_manager);
    proposal_manager.expect_wrap_propose_block().times(1).return_once(
        move |_proposal_id, _block_hash, _deadline, tx_sender, _tx_provider| {
            simulate_build_block_proposal(tx_sender, txs_to_stream).boxed()
        },
    );

    let mut batcher = batcher(proposal_manager);

    batcher.start_height(StartHeightInput { height: INITIAL_HEIGHT }).await.unwrap();
    batcher
        .propose_block(ProposeBlockInput {
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
            content: GetProposalContent::Finished(commitment)
        } if commitment == proposal_commitment()
    );

    let exhausted =
        batcher.get_proposal_content(GetProposalContentInput { proposal_id: PROPOSAL_ID }).await;
    assert_matches!(exhausted, Err(BatcherError::ProposalNotFound { .. }));
}

#[rstest]
#[tokio::test]
async fn get_content_from_unknown_proposal() {
    let mut proposal_manager = MockProposalManagerTraitWrapper::new();
    proposal_manager.expect_wrap_await_proposal_commitment().times(0);

    let mut batcher = batcher(proposal_manager);

    let get_proposal_content_input = GetProposalContentInput { proposal_id: PROPOSAL_ID };
    let result = batcher.get_proposal_content(get_proposal_content_input).await;
    assert_eq!(result, Err(BatcherError::ProposalNotFound { proposal_id: PROPOSAL_ID }));
}

#[rstest]
#[tokio::test]
async fn decision_reached(
    batcher_config: BatcherConfig,
    storage_reader: MockBatcherStorageReaderTrait,
    mut storage_writer: MockBatcherStorageWriterTrait,
    mut mempool_client: MockMempoolClient,
) {
    let expected_state_diff = ThinStateDiff::default();
    let state_diff_clone = expected_state_diff.clone();
    let expected_proposal_commitment = ProposalCommitment::default();
    let tx_hashes = test_tx_hashes(0..5);
    let tx_hashes_clone = tx_hashes.clone();
    let address_to_nonce = test_contract_nonces(0..3);
    let nonces_clone = address_to_nonce.clone();

    let mut proposal_manager = MockProposalManagerTraitWrapper::new();
    proposal_manager.expect_wrap_take_proposal_result().times(1).with(eq(PROPOSAL_ID)).return_once(
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
    batcher.decision_reached(DecisionReachedInput { proposal_id: PROPOSAL_ID }).await.unwrap();
}

#[rstest]
#[tokio::test]
async fn decision_reached_no_executed_proposal() {
    let expected_error = BatcherError::ExecutedProposalNotFound { proposal_id: PROPOSAL_ID };

    let mut proposal_manager = MockProposalManagerTraitWrapper::new();
    proposal_manager.expect_wrap_take_proposal_result().times(1).with(eq(PROPOSAL_ID)).return_once(
        |proposal_id| {
            async move { Err(GetProposalResultError::ProposalDoesNotExist { proposal_id }) }.boxed()
        },
    );

    let mut batcher = batcher(proposal_manager);
    let decision_reached_result =
        batcher.decision_reached(DecisionReachedInput { proposal_id: PROPOSAL_ID }).await;
    assert_eq!(decision_reached_result, Err(expected_error));
}

async fn simulate_build_block_proposal(
    tx_sender: tokio::sync::mpsc::UnboundedSender<Transaction>,
    txs: Vec<Transaction>,
) -> Result<(), GenerateProposalError> {
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

    fn wrap_propose_block(
        &mut self,
        proposal_id: ProposalId,
        retrospective_block_hash: Option<BlockHashAndNumber>,
        deadline: tokio::time::Instant,
        output_content_sender: tokio::sync::mpsc::UnboundedSender<Transaction>,
        tx_provider: ProposeTransactionProvider,
    ) -> BoxFuture<'_, Result<(), GenerateProposalError>>;

    fn wrap_validate_block(
        &mut self,
        proposal_id: ProposalId,
        retrospective_block_hash: Option<BlockHashAndNumber>,
        deadline: tokio::time::Instant,
        tx_provider: ValidateTransactionProvider,
    ) -> BoxFuture<'_, Result<(), GenerateProposalError>>;

    fn wrap_take_proposal_result(
        &mut self,
        proposal_id: ProposalId,
    ) -> BoxFuture<'_, ProposalResult<ProposalOutput>>;

    fn wrap_get_proposal_status(
        &self,
        proposal_id: ProposalId,
    ) -> BoxFuture<'_, InternalProposalStatus>;

    fn wrap_await_proposal_commitment(
        &self,
        proposal_id: ProposalId,
    ) -> BoxFuture<'_, ProposalResult<ProposalCommitment>>;

    fn wrap_abort_proposal(&mut self, proposal_id: ProposalId) -> BoxFuture<'_, ()>;
}

#[async_trait]
impl<T: ProposalManagerTraitWrapper> ProposalManagerTrait for T {
    async fn start_height(&mut self, height: BlockNumber) -> Result<(), StartHeightError> {
        self.wrap_start_height(height).await
    }

    async fn propose_block(
        &mut self,
        proposal_id: ProposalId,
        retrospective_block_hash: Option<BlockHashAndNumber>,
        deadline: tokio::time::Instant,
        output_content_sender: tokio::sync::mpsc::UnboundedSender<Transaction>,
        tx_provider: ProposeTransactionProvider,
    ) -> Result<(), GenerateProposalError> {
        self.wrap_propose_block(
            proposal_id,
            retrospective_block_hash,
            deadline,
            output_content_sender,
            tx_provider,
        )
        .await
    }

    async fn validate_block(
        &mut self,
        proposal_id: ProposalId,
        retrospective_block_hash: Option<BlockHashAndNumber>,
        deadline: tokio::time::Instant,
        tx_provider: ValidateTransactionProvider,
    ) -> Result<(), GenerateProposalError> {
        self.wrap_validate_block(proposal_id, retrospective_block_hash, deadline, tx_provider).await
    }

    async fn take_proposal_result(
        &mut self,
        proposal_id: ProposalId,
    ) -> ProposalResult<ProposalOutput> {
        self.wrap_take_proposal_result(proposal_id).await
    }

    async fn get_proposal_status(&self, proposal_id: ProposalId) -> InternalProposalStatus {
        self.wrap_get_proposal_status(proposal_id).await
    }

    async fn await_proposal_commitment(
        &mut self,
        proposal_id: ProposalId,
    ) -> ProposalResult<ProposalCommitment> {
        self.wrap_await_proposal_commitment(proposal_id).await
    }

    async fn abort_proposal(&mut self, proposal_id: ProposalId) {
        self.wrap_abort_proposal(proposal_id).await
    }
}

fn test_tx_hashes(range: std::ops::Range<u128>) -> HashSet<TransactionHash> {
    range.map(|i| TransactionHash(felt!(i))).collect()
}

fn test_contract_nonces(range: std::ops::Range<u128>) -> HashMap<ContractAddress, Nonce> {
    HashMap::from_iter(range.map(|i| (contract_address!(i), nonce!(i))))
}
