use std::sync::Arc;

use assert_matches::assert_matches;
use rstest::{fixture, rstest};
use starknet_api::block::BlockNumber;
use starknet_api::executable_transaction::Transaction;
use starknet_batcher_types::batcher_types::ProposalId;
use starknet_mempool_types::communication::MockMempoolClient;

use crate::batcher::MockBatcherStorageReaderTrait;
use crate::block_builder::{
    BlockBuilderResult,
    BlockBuilderTrait,
    BlockExecutionArtifacts,
    MockBlockBuilderFactoryTrait,
    MockBlockBuilderTrait,
};
use crate::proposal_manager::{
    BuildProposalError,
    ProposalManager,
    ProposalManagerTrait,
    StartHeightError,
};

const INITIAL_HEIGHT: BlockNumber = BlockNumber(3);
const BLOCK_GENERATION_TIMEOUT: tokio::time::Duration = tokio::time::Duration::from_secs(1);

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
    mempool_client: MockMempoolClient,
    block_builder_factory: MockBlockBuilderFactoryTrait,
    storage_reader: MockBatcherStorageReaderTrait,
) -> ProposalManager {
    ProposalManager::new(
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
#[tokio::test]
async fn start_height(
    mut proposal_manager: ProposalManager,
    #[case] height: BlockNumber,
    #[case] expected_result: Result<(), StartHeightError>,
) {
    let result = proposal_manager.start_height(height).await;
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
    mut block_builder_factory: MockBlockBuilderFactoryTrait,
    mempool_client: MockMempoolClient,
    storage_reader: MockBatcherStorageReaderTrait,
    output_streaming: (
        tokio::sync::mpsc::UnboundedSender<Transaction>,
        tokio::sync::mpsc::UnboundedReceiver<Transaction>,
    ),
) {
    block_builder_factory
        .expect_create_block_builder()
        .once()
        .returning(move |_, _| simulate_build_block());

    let mut proposal_manager = ProposalManager::new(
        Arc::new(mempool_client),
        Arc::new(block_builder_factory),
        Arc::new(storage_reader),
    );

    proposal_manager.start_height(INITIAL_HEIGHT).await.unwrap();

    proposal_manager
        .build_block_proposal(ProposalId(0), None, arbitrary_deadline(), output_streaming.0)
        .await
        .unwrap();

    proposal_manager.await_active_proposal().await;
}

#[rstest]
#[tokio::test]
async fn consecutive_proposal_generations_success(
    mut block_builder_factory: MockBlockBuilderFactoryTrait,
    mempool_client: MockMempoolClient,
    storage_reader: MockBatcherStorageReaderTrait,
) {
    block_builder_factory
        .expect_create_block_builder()
        .times(2)
        .returning(move |_, _| simulate_build_block());

    let mut proposal_manager = ProposalManager::new(
        Arc::new(mempool_client),
        Arc::new(block_builder_factory),
        Arc::new(storage_reader),
    );

    proposal_manager.start_height(INITIAL_HEIGHT).await.unwrap();

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

// This test checks that trying to generate a proposal while another one is being generated will
// fail. First the test will generate a new proposal that takes a very long time, and during
// that time it will send another build proposal request.
#[rstest]
#[tokio::test]
async fn multiple_proposals_generation_fail(
    mut block_builder_factory: MockBlockBuilderFactoryTrait,
    mempool_client: MockMempoolClient,
    storage_reader: MockBatcherStorageReaderTrait,
) {
    // Generate a block builder with a very long build block operation.
    block_builder_factory
        .expect_create_block_builder()
        .once()
        .returning(|_, _| simulate_build_block_with_delay());

    let mut proposal_manager = ProposalManager::new(
        Arc::new(mempool_client),
        Arc::new(block_builder_factory),
        Arc::new(storage_reader),
    );

    proposal_manager.start_height(INITIAL_HEIGHT).await.unwrap();

    // Build a proposal that will take a very long time to finish.
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

#[rstest]
#[tokio::test]
async fn test_take_proposal_result_no_active_proposal(
    mut block_builder_factory: MockBlockBuilderFactoryTrait,
    mempool_client: MockMempoolClient,
    storage_reader: MockBatcherStorageReaderTrait,
) {
    let (output_sender_0, _rec_0) = output_streaming();
    let (output_sender_1, _rec_1) = output_streaming();
    block_builder_factory
        .expect_create_block_builder()
        .once()
        .returning(move |_, _| simulate_build_block());

    block_builder_factory
        .expect_create_block_builder()
        .once()
        .returning(move |_, _| simulate_build_block());

    let mut proposal_manager = ProposalManager::new(
        Arc::new(mempool_client),
        Arc::new(block_builder_factory),
        Arc::new(storage_reader),
    );

    proposal_manager.start_height(INITIAL_HEIGHT).await.unwrap();

    proposal_manager
        .build_block_proposal(ProposalId(0), None, arbitrary_deadline(), output_sender_0)
        .await
        .unwrap();

    // Make sure the first proposal generated successfully.
    proposal_manager.await_active_proposal().await;

    proposal_manager
        .build_block_proposal(ProposalId(1), None, arbitrary_deadline(), output_sender_1)
        .await
        .unwrap();

    // Make sure the proposal generated successfully.
    proposal_manager.await_active_proposal().await;

    proposal_manager.take_proposal_result(ProposalId(0)).await.unwrap();
    proposal_manager.take_proposal_result(ProposalId(1)).await.unwrap();
}

fn arbitrary_deadline() -> tokio::time::Instant {
    tokio::time::Instant::now() + BLOCK_GENERATION_TIMEOUT
}

// This function simulates a long build block operation. This is required for a test that tries
// to run other operations while a block is being built.
fn simulate_build_block_with_delay() -> BlockBuilderResult<Box<dyn BlockBuilderTrait>> {
    let mut mock_block_builder = MockBlockBuilderTrait::new();
    mock_block_builder.expect_build_block().return_once(move |_, _, _| {
        std::thread::sleep(BLOCK_GENERATION_TIMEOUT * 10);
        Ok(BlockExecutionArtifacts::create_for_testing())
    });
    Ok(Box::new(mock_block_builder))
}

fn simulate_build_block() -> BlockBuilderResult<Box<dyn BlockBuilderTrait>> {
    let mut mock_block_builder = MockBlockBuilderTrait::new();
    mock_block_builder
        .expect_build_block()
        .return_once(move |_, _, _| Ok(BlockExecutionArtifacts::create_for_testing()));
    Ok(Box::new(mock_block_builder))
}
