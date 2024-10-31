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
    GetProposalResultError,
    ProposalManager,
    ProposalManagerTrait,
    ProposalOutput,
    StartHeightError,
};

const INITIAL_HEIGHT: BlockNumber = BlockNumber(3);
const BLOCK_GENERATION_TIMEOUT: tokio::time::Duration = tokio::time::Duration::from_secs(1);

#[fixture]
fn output_streaming() -> (
    tokio::sync::mpsc::UnboundedSender<Transaction>,
    tokio::sync::mpsc::UnboundedReceiver<Transaction>,
) {
    let (output_content_sender, output_content_receiver) = tokio::sync::mpsc::unbounded_channel();
    (output_content_sender, output_content_receiver)
}

struct MockDependencies {
    block_builder_factory: MockBlockBuilderFactoryTrait,
    mempool_client: MockMempoolClient,
    storage_reader: MockBatcherStorageReaderTrait,
}

impl MockDependencies {
    fn expect_build_block(&mut self, times: usize) {
        let simulate_build_block = || -> BlockBuilderResult<Box<dyn BlockBuilderTrait>> {
            let mut mock_block_builder = MockBlockBuilderTrait::new();
            mock_block_builder
                .expect_build_block()
                .return_once(move |_, _, _| Ok(BlockExecutionArtifacts::create_for_testing()));
            Ok(Box::new(mock_block_builder))
        };

        self.block_builder_factory
            .expect_create_block_builder()
            .times(times)
            .returning(move |_, _| simulate_build_block());
    }

    // This function simulates a long build block operation. This is required for a test that
    // tries to run other operations while a block is being built.
    fn expect_long_build_block(&mut self, times: usize) {
        let simulate_long_build_block = || -> BlockBuilderResult<Box<dyn BlockBuilderTrait>> {
            let mut mock_block_builder = MockBlockBuilderTrait::new();
            mock_block_builder.expect_build_block().return_once(move |_, _, _| {
                std::thread::sleep(BLOCK_GENERATION_TIMEOUT * 10);
                Ok(BlockExecutionArtifacts::create_for_testing())
            });
            Ok(Box::new(mock_block_builder))
        };

        self.block_builder_factory
            .expect_create_block_builder()
            .times(times)
            .returning(move |_, _| simulate_long_build_block());
    }
}

#[fixture]
fn mock_dependencies() -> MockDependencies {
    let mut storage_reader = MockBatcherStorageReaderTrait::new();
    storage_reader.expect_height().returning(|| Ok(INITIAL_HEIGHT));
    MockDependencies {
        block_builder_factory: MockBlockBuilderFactoryTrait::new(),
        mempool_client: MockMempoolClient::new(),
        storage_reader,
    }
}

fn init_proposal_manager(mock_dependencies: MockDependencies) -> ProposalManager {
    ProposalManager::new(
        Arc::new(mock_dependencies.mempool_client),
        Arc::new(mock_dependencies.block_builder_factory),
        Arc::new(mock_dependencies.storage_reader),
    )
}

fn proposal_deadline() -> tokio::time::Instant {
    tokio::time::Instant::now() + BLOCK_GENERATION_TIMEOUT
}

async fn build_and_await_block_proposal(
    proposal_manager: &mut ProposalManager,
    proposal_id: ProposalId,
) {
    let (output_sender, _receiver) = output_streaming();
    proposal_manager
        .build_block_proposal(proposal_id, None, proposal_deadline(), output_sender)
        .await
        .unwrap();

    assert!(proposal_manager.await_active_proposal().await);
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
    mock_dependencies: MockDependencies,
    #[case] height: BlockNumber,
    #[case] expected_result: Result<(), StartHeightError>,
) {
    let mut proposal_manager = init_proposal_manager(mock_dependencies);
    let result = proposal_manager.start_height(height).await;
    // Unfortunatelly ProposalManagerError doesn't implement PartialEq.
    assert_eq!(format!("{:?}", result), format!("{:?}", expected_result));
}

#[rstest]
#[tokio::test]
async fn proposal_generation_fails_without_start_height(
    mock_dependencies: MockDependencies,
    output_streaming: (
        tokio::sync::mpsc::UnboundedSender<Transaction>,
        tokio::sync::mpsc::UnboundedReceiver<Transaction>,
    ),
) {
    let mut proposal_manager = init_proposal_manager(mock_dependencies);
    let err = proposal_manager
        .build_block_proposal(ProposalId(0), None, proposal_deadline(), output_streaming.0)
        .await;
    assert_matches!(err, Err(BuildProposalError::NoActiveHeight));
}

#[rstest]
#[tokio::test]
async fn proposal_generation_success(mut mock_dependencies: MockDependencies) {
    mock_dependencies.expect_build_block(1);

    let mut proposal_manager = init_proposal_manager(mock_dependencies);

    proposal_manager.start_height(INITIAL_HEIGHT).await.unwrap();
    build_and_await_block_proposal(&mut proposal_manager, ProposalId(0)).await;
}

#[rstest]
#[tokio::test]
async fn consecutive_proposal_generations_success(mut mock_dependencies: MockDependencies) {
    mock_dependencies.expect_build_block(2);

    let mut proposal_manager = init_proposal_manager(mock_dependencies);

    proposal_manager.start_height(INITIAL_HEIGHT).await.unwrap();

    // Generate two consecutive proposals (awaiting on them to make sure they finished
    // successfully).
    build_and_await_block_proposal(&mut proposal_manager, ProposalId(0)).await;
    build_and_await_block_proposal(&mut proposal_manager, ProposalId(1)).await;
}

// This test checks that trying to generate a proposal while another one is being generated will
// fail. First the test will generate a new proposal that takes a very long time, and during
// that time it will send another build proposal request.
#[rstest]
#[tokio::test]
async fn multiple_proposals_generation_fail(mut mock_dependencies: MockDependencies) {
    // Generate a block builder with a very long build block operation.
    mock_dependencies.expect_long_build_block(1);

    let mut proposal_manager = init_proposal_manager(mock_dependencies);

    proposal_manager.start_height(INITIAL_HEIGHT).await.unwrap();

    // Build a proposal that will take a very long time to finish.
    let (output_sender_0, _rec_0) = output_streaming();
    proposal_manager
        .build_block_proposal(ProposalId(0), None, proposal_deadline(), output_sender_0)
        .await
        .unwrap();

    // Try to generate another proposal while the first one is still being generated.
    let (output_sender_1, _rec_1) = output_streaming();
    let another_generate_request = proposal_manager
        .build_block_proposal(ProposalId(1), None, proposal_deadline(), output_sender_1)
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
async fn test_take_proposal_result_no_active_proposal(mut mock_dependencies: MockDependencies) {
    mock_dependencies.expect_build_block(1);

    let mut proposal_manager = init_proposal_manager(mock_dependencies);

    proposal_manager.start_height(INITIAL_HEIGHT).await.unwrap();

    build_and_await_block_proposal(&mut proposal_manager, ProposalId(0)).await;

    let expected_proposal_output =
        ProposalOutput::from(BlockExecutionArtifacts::create_for_testing());
    assert_eq!(
        proposal_manager.take_proposal_result(ProposalId(0)).await.unwrap(),
        expected_proposal_output
    );
    assert_matches!(
        proposal_manager.take_proposal_result(ProposalId(0)).await,
        Err(GetProposalResultError::ProposalDoesNotExist { .. })
    );
}

#[rstest]
#[tokio::test]
async fn test_abort_and_restart_height(mut mock_dependencies: MockDependencies) {
    mock_dependencies.expect_build_block(1);
    mock_dependencies.expect_long_build_block(1);

    // Start a new height and create a proposal.
    let (output_tx_sender, _receiver) = output_streaming();
    let mut proposal_manager = init_proposal_manager(mock_dependencies);
    proposal_manager.start_height(INITIAL_HEIGHT).await.unwrap();
    build_and_await_block_proposal(&mut proposal_manager, ProposalId(0)).await;

    // Start a new proposal, which will remain active.
    assert!(
        proposal_manager
            .build_block_proposal(ProposalId(1), None, proposal_deadline(), output_tx_sender)
            .await
            .is_ok()
    );

    // Restart the same height. This should abort and delete all existing proposals.
    assert!(proposal_manager.start_height(INITIAL_HEIGHT).await.is_ok());

    // Make sure executed proposals are deleted.
    assert_matches!(
        proposal_manager.take_proposal_result(ProposalId(0)).await,
        Err(GetProposalResultError::ProposalDoesNotExist { .. })
    );

    // Make sure there is no active proposal.
    // TODO: uncommomment once the abort is implemented. This line will panic now.
    // assert!(!proposal_manager.await_active_proposal().await);
}
