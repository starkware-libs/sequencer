use std::sync::Arc;
use std::vec;

use assert_matches::assert_matches;
use async_trait::async_trait;
use futures::future::BoxFuture;
use futures::FutureExt;
use mockall::automock;
use rstest::{fixture, rstest};
use starknet_api::block::BlockNumber;
use starknet_api::executable_transaction::Transaction;
use starknet_batcher_types::batcher_types::ProposalId;
use starknet_mempool_types::communication::MockMempoolClient;
use tokio_stream::StreamExt;

use crate::batcher::MockBatcherStorageReaderTrait;
use crate::block_builder::{
    BlockBuilderResult,
    BlockBuilderTrait,
    BlockExecutionArtifacts,
    MockBlockBuilderFactoryTrait,
};
use crate::proposal_manager::{
    BuildProposalError,
    InputTxStream,
    ProposalManager,
    ProposalManagerConfig,
    ProposalManagerTrait,
    StartHeightError,
};
use crate::test_utils::test_txs;

const INITIAL_HEIGHT: BlockNumber = BlockNumber(3);

#[fixture]
fn proposal_manager_config() -> ProposalManagerConfig {
    ProposalManagerConfig::default()
}

// TODO: Figure out how to pass expectations to the mock.
#[fixture]
fn block_builder_factory() -> MockBlockBuilderFactoryTrait {
    MockBlockBuilderFactoryTrait::new()
}

// TODO: Figure out how to pass expectations to the mock.
#[fixture]
fn mempool_client() -> MockMempoolClient {
    MockMempoolClient::new()
}

#[fixture]
fn output_streaming() -> (
    tokio::sync::mpsc::UnboundedSender<Transaction>,
    tokio::sync::mpsc::UnboundedReceiver<Transaction>,
) {
    let (output_content_sender, output_content_receiver) = tokio::sync::mpsc::unbounded_channel();
    (output_content_sender, output_content_receiver)
}

#[fixture]
fn storage_reader() -> MockBatcherStorageReaderTrait {
    let mut storage = MockBatcherStorageReaderTrait::new();
    storage.expect_height().returning(|| Ok(INITIAL_HEIGHT));
    storage
}

#[fixture]
fn proposal_manager(
    proposal_manager_config: ProposalManagerConfig,
    mempool_client: MockMempoolClient,
    block_builder_factory: MockBlockBuilderFactoryTrait,
    storage_reader: MockBatcherStorageReaderTrait,
) -> ProposalManager {
    ProposalManager::new(
        proposal_manager_config,
        Arc::new(mempool_client),
        Arc::new(block_builder_factory),
        Arc::new(storage_reader),
    )
}

#[rstest]
#[case::height_already_passed(
    INITIAL_HEIGHT.prev().unwrap(),
    Result::Err(StartHeightError::HeightAlreadyPassed {
        storage_height: INITIAL_HEIGHT,
        requested_height: INITIAL_HEIGHT.prev().unwrap()
    }
))]
#[case::happy(
    INITIAL_HEIGHT,
    Result::Ok(())
)]
#[case::storage_not_synced(
    INITIAL_HEIGHT.unchecked_next(),
    Result::Err(StartHeightError::StorageNotSynced {
        storage_height: INITIAL_HEIGHT,
        requested_height: INITIAL_HEIGHT.unchecked_next()
    }
))]
fn start_height(
    mut proposal_manager: ProposalManager,
    #[case] height: BlockNumber,
    #[case] expected_result: Result<(), StartHeightError>,
) {
    let result = proposal_manager.start_height(height);
    // Unfortunatelly ProposalManagerError doesn't implement PartialEq.
    assert_eq!(format!("{:?}", result), format!("{:?}", expected_result));
}

#[rstest]
#[tokio::test]
async fn proposal_generation_fails_without_start_height(
    mut proposal_manager: ProposalManager,
    output_streaming: (
        tokio::sync::mpsc::UnboundedSender<Transaction>,
        tokio::sync::mpsc::UnboundedReceiver<Transaction>,
    ),
) {
    let err = proposal_manager
        .build_block_proposal(ProposalId(0), None, arbitrary_deadline(), output_streaming.0)
        .await;
    assert_matches!(err, Err(BuildProposalError::NoActiveHeight));
}

#[rstest]
#[tokio::test]
async fn proposal_generation_success(
    proposal_manager_config: ProposalManagerConfig,
    mut block_builder_factory: MockBlockBuilderFactoryTrait,
    mut mempool_client: MockMempoolClient,
    storage_reader: MockBatcherStorageReaderTrait,
    output_streaming: (
        tokio::sync::mpsc::UnboundedSender<Transaction>,
        tokio::sync::mpsc::UnboundedReceiver<Transaction>,
    ),
) {
    let n_txs = 2 * proposal_manager_config.max_txs_per_mempool_request;
    block_builder_factory
        .expect_create_block_builder()
        .once()
        .returning(move |_, _| simulate_build_block(Some(n_txs)));

    mempool_client.expect_get_txs().once().returning(|max_n_txs| Ok(test_txs(0..max_n_txs)));

    mempool_client
        .expect_get_txs()
        .once()
        .returning(|max_n_txs| Ok(test_txs(max_n_txs..2 * max_n_txs)));

    mempool_client.expect_get_txs().returning(|_| Ok(vec![]));

    let mut proposal_manager = ProposalManager::new(
        proposal_manager_config.clone(),
        Arc::new(mempool_client),
        Arc::new(block_builder_factory),
        Arc::new(storage_reader),
    );

    proposal_manager.start_height(INITIAL_HEIGHT).unwrap();

    proposal_manager
        .build_block_proposal(ProposalId(0), None, arbitrary_deadline(), output_streaming.0)
        .await
        .unwrap();

    proposal_manager.await_active_proposal().await;
}

#[rstest]
#[tokio::test]
async fn consecutive_proposal_generations_success(
    proposal_manager_config: ProposalManagerConfig,
    mut block_builder_factory: MockBlockBuilderFactoryTrait,
    mut mempool_client: MockMempoolClient,
    storage_reader: MockBatcherStorageReaderTrait,
) {
    let n_txs = proposal_manager_config.max_txs_per_mempool_request;
    block_builder_factory
        .expect_create_block_builder()
        .times(2)
        .returning(move |_, _| simulate_build_block(Some(n_txs)));

    let expected_txs = test_txs(0..proposal_manager_config.max_txs_per_mempool_request);
    let mempool_txs = expected_txs.clone();
    mempool_client.expect_get_txs().returning(move |_max_n_txs| Ok(mempool_txs.clone()));

    let mut proposal_manager = ProposalManager::new(
        proposal_manager_config.clone(),
        Arc::new(mempool_client),
        Arc::new(block_builder_factory),
        Arc::new(storage_reader),
    );

    proposal_manager.start_height(INITIAL_HEIGHT).unwrap();

    let (output_sender_0, _rec_0) = output_streaming();
    proposal_manager
        .build_block_proposal(ProposalId(0), None, arbitrary_deadline(), output_sender_0)
        .await
        .unwrap();

    // Make sure the first proposal generated successfully.
    proposal_manager.await_active_proposal().await;

    let (output_sender_1, _rec_1) = output_streaming();
    proposal_manager
        .build_block_proposal(ProposalId(1), None, arbitrary_deadline(), output_sender_1)
        .await
        .unwrap();

    // Make sure the proposal generated successfully.
    proposal_manager.await_active_proposal().await;
}

#[rstest]
#[tokio::test]
async fn multiple_proposals_generation_fail(
    proposal_manager_config: ProposalManagerConfig,
    mut block_builder_factory: MockBlockBuilderFactoryTrait,
    mut mempool_client: MockMempoolClient,
    storage_reader: MockBatcherStorageReaderTrait,
) {
    // The block builder will never stop.
    block_builder_factory
        .expect_create_block_builder()
        .once()
        .returning(|_, _| simulate_build_block(None));

    mempool_client.expect_get_txs().returning(|_| Ok(vec![]));

    let mut proposal_manager = ProposalManager::new(
        proposal_manager_config.clone(),
        Arc::new(mempool_client),
        Arc::new(block_builder_factory),
        Arc::new(storage_reader),
    );

    proposal_manager.start_height(INITIAL_HEIGHT).unwrap();

    // A proposal that will never finish.
    let (output_sender_0, _rec_0) = output_streaming();
    proposal_manager
        .build_block_proposal(ProposalId(0), None, arbitrary_deadline(), output_sender_0)
        .await
        .unwrap();

    // Try to generate another proposal while the first one is still being generated.
    let (output_sender_1, _rec_1) = output_streaming();
    let another_generate_request = proposal_manager
        .build_block_proposal(ProposalId(1), None, arbitrary_deadline(), output_sender_1)
        .await;
    assert_matches!(
        another_generate_request,
        Err(BuildProposalError::AlreadyGeneratingProposal {
            current_generating_proposal_id,
            new_proposal_id
        }) if current_generating_proposal_id == ProposalId(0) && new_proposal_id == ProposalId(1)
    );
}

fn arbitrary_deadline() -> tokio::time::Instant {
    const GENERATION_TIMEOUT: tokio::time::Duration = tokio::time::Duration::from_secs(1);
    tokio::time::Instant::now() + GENERATION_TIMEOUT
}

fn simulate_build_block(n_txs: Option<usize>) -> BlockBuilderResult<Box<dyn BlockBuilderTrait>> {
    let mut mock_block_builder = MockBlockBuilderTraitWrapper::new();
    mock_block_builder.expect_wrap_build_block().return_once(
        move |deadline, mempool_tx_stream, output_content_sender| {
            simulate_block_builder(deadline, mempool_tx_stream, output_content_sender, n_txs)
                .boxed()
        },
    );
    Ok(Box::new(mock_block_builder))
}

async fn simulate_block_builder(
    _deadline: tokio::time::Instant,
    mempool_tx_stream: InputTxStream,
    output_sender: tokio::sync::mpsc::UnboundedSender<Transaction>,
    n_txs_to_take: Option<usize>,
) -> BlockBuilderResult<BlockExecutionArtifacts> {
    let mut mempool_tx_stream = mempool_tx_stream.take(n_txs_to_take.unwrap_or(usize::MAX));
    while let Some(tx) = mempool_tx_stream.next().await {
        output_sender.send(tx).unwrap();
    }

    Ok(BlockExecutionArtifacts::default())
}

// A wrapper trait to allow mocking the BlockBuilderTrait in tests.
#[automock]
trait BlockBuilderTraitWrapper: Send + Sync {
    // Equivalent to: async fn build_block(&self, deadline: tokio::time::Instant);
    fn wrap_build_block(
        &self,
        deadline: tokio::time::Instant,
        tx_stream: InputTxStream,
        output_content_sender: tokio::sync::mpsc::UnboundedSender<Transaction>,
    ) -> BoxFuture<'_, BlockBuilderResult<BlockExecutionArtifacts>>;
}

#[async_trait]
impl<T: BlockBuilderTraitWrapper> BlockBuilderTrait for T {
    async fn build_block(
        &mut self,
        deadline: tokio::time::Instant,
        tx_stream: InputTxStream,
        output_content_sender: tokio::sync::mpsc::UnboundedSender<Transaction>,
    ) -> BlockBuilderResult<BlockExecutionArtifacts> {
        self.wrap_build_block(deadline, tx_stream, output_content_sender).await
    }
}
