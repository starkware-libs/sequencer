use std::sync::Arc;

use assert_matches::assert_matches;
use async_trait::async_trait;
use futures::future::BoxFuture;
use futures::FutureExt;
#[cfg(test)]
use mockall::automock;
use rstest::{fixture, rstest};
use starknet_api::block::BlockNumber;
use starknet_api::executable_transaction::Transaction;
use starknet_batcher_types::batcher_types::ProposalId;
use starknet_mempool_types::communication::MockMempoolClient;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

use crate::batcher::MockBatcherStorageReaderTrait;
use crate::block_builder::{
    BlockBuilderResult,
    BlockBuilderTrait,
    BlockExecutionArtifacts,
    MockBlockBuilderFactoryTrait,
};
use crate::proposal_manager::{
    InputTxStream,
    ProposalManager,
    ProposalManagerConfig,
    ProposalManagerError,
    ProposalManagerResult,
    ProposalManagerTrait,
};
use crate::test_utils::test_txs;

pub type OutputTxStream = ReceiverStream<Transaction>;

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
fn output_streaming() -> (tokio::sync::mpsc::Sender<Transaction>, OutputTxStream) {
    const OUTPUT_CONTENT_BUFFER_SIZE: usize = 100;
    let (output_content_sender, output_content_receiver) =
        tokio::sync::mpsc::channel(OUTPUT_CONTENT_BUFFER_SIZE);
    let stream = tokio_stream::wrappers::ReceiverStream::new(output_content_receiver);
    (output_content_sender, stream)
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
    ProposalManagerResult::Err(ProposalManagerError::HeightAlreadyPassed {
        storage_height: INITIAL_HEIGHT,
        requested_height: INITIAL_HEIGHT.prev().unwrap()
    }
))]
#[case::happy(
    INITIAL_HEIGHT,
    ProposalManagerResult::Ok(())
)]
#[case::storage_not_synced(
    INITIAL_HEIGHT.unchecked_next(),
    ProposalManagerResult::Err(ProposalManagerError::StorageNotSynced {
        storage_height: INITIAL_HEIGHT,
        requested_height: INITIAL_HEIGHT.unchecked_next()
    }
))]
fn start_height(
    mut proposal_manager: ProposalManager,
    #[case] height: BlockNumber,
    #[case] expected_result: ProposalManagerResult<()>,
) {
    let result = proposal_manager.start_height(height);
    // Unfortunatelly ProposalManagerError doesn't implement PartialEq.
    assert_eq!(format!("{:?}", result), format!("{:?}", expected_result));
}

#[rstest]
#[tokio::test]
async fn proposal_generation_fails_without_start_height(
    mut proposal_manager: ProposalManager,
    output_streaming: (tokio::sync::mpsc::Sender<Transaction>, OutputTxStream),
) {
    let (output_content_sender, _stream) = output_streaming;
    let err = proposal_manager
        .build_block_proposal(ProposalId(0), None, arbitrary_deadline(), output_content_sender)
        .await;
    assert_matches!(err, Err(ProposalManagerError::NoActiveHeight));
}

#[rstest]
#[tokio::test]
async fn proposal_generation_success(
    proposal_manager_config: ProposalManagerConfig,
    mut block_builder_factory: MockBlockBuilderFactoryTrait,
    mut mempool_client: MockMempoolClient,
    output_streaming: (tokio::sync::mpsc::Sender<Transaction>, OutputTxStream),
    storage_reader: MockBatcherStorageReaderTrait,
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

    let (output_content_sender, stream) = output_streaming;
    proposal_manager
        .build_block_proposal(ProposalId(0), None, arbitrary_deadline(), output_content_sender)
        .await
        .unwrap();

    assert_matches!(proposal_manager.await_active_proposal().await, Some(Ok(())));
    let proposal_content: Vec<_> = stream.collect().await;
    assert_eq!(proposal_content, test_txs(0..n_txs));
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

    let (output_content_sender, stream) = output_streaming();
    proposal_manager
        .build_block_proposal(ProposalId(0), None, arbitrary_deadline(), output_content_sender)
        .await
        .unwrap();

    // Make sure the first proposal generated successfully.
    assert_matches!(proposal_manager.await_active_proposal().await, Some(Ok(())));
    let v: Vec<_> = stream.collect().await;
    assert_eq!(v, expected_txs);

    let (output_content_sender, stream) = output_streaming();
    proposal_manager
        .build_block_proposal(ProposalId(1), None, arbitrary_deadline(), output_content_sender)
        .await
        .unwrap();

    // Make sure the proposal generated successfully.
    assert_matches!(proposal_manager.await_active_proposal().await, Some(Ok(())));
    let proposal_content: Vec<_> = stream.collect().await;
    assert_eq!(proposal_content, expected_txs);
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
    let (output_content_sender, _stream) = output_streaming();
    proposal_manager
        .build_block_proposal(ProposalId(0), None, arbitrary_deadline(), output_content_sender)
        .await
        .unwrap();

    // Try to generate another proposal while the first one is still being generated.
    let (another_output_content_sender, _another_stream) = output_streaming();
    let another_generate_request = proposal_manager
        .build_block_proposal(
            ProposalId(1),
            None,
            arbitrary_deadline(),
            another_output_content_sender,
        )
        .await;
    assert_matches!(
        another_generate_request,
        Err(ProposalManagerError::AlreadyGeneratingProposal {
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
    mock_block_builder.expect_build_block_wrapper().return_once(
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
    output_sender: tokio::sync::mpsc::Sender<Transaction>,
    n_txs_to_take: Option<usize>,
) -> BlockBuilderResult<BlockExecutionArtifacts> {
    let mut mempool_tx_stream = mempool_tx_stream.take(n_txs_to_take.unwrap_or(usize::MAX));
    while let Some(tx) = mempool_tx_stream.next().await {
        output_sender.send(tx).await.unwrap();
    }

    Ok(BlockExecutionArtifacts::default())
}

// A wrapper trait to allow mocking the BlockBuilderTrait in tests.
#[cfg_attr(test, automock)]
trait BlockBuilderTraitWrapper: Send + Sync {
    // Equivalent to: async fn build_block(&self, deadline: tokio::time::Instant);
    fn build_block_wrapper(
        &self,
        deadline: tokio::time::Instant,
        tx_stream: InputTxStream,
        output_content_sender: tokio::sync::mpsc::Sender<Transaction>,
    ) -> BoxFuture<'_, BlockBuilderResult<BlockExecutionArtifacts>>;
}

#[async_trait]
impl<T: BlockBuilderTraitWrapper> BlockBuilderTrait for T {
    async fn build_block(
        &mut self,
        deadline: tokio::time::Instant,
        tx_stream: InputTxStream,
        output_content_sender: tokio::sync::mpsc::Sender<Transaction>,
    ) -> BlockBuilderResult<BlockExecutionArtifacts> {
        self.build_block_wrapper(deadline, tx_stream, output_content_sender).await
    }
}
