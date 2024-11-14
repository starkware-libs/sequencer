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
    GenerateProposalError,
    GetProposalResultError,
    ProposalManager,
    ProposalManagerTrait,
    ProposalOutput,
    StartHeightError,
};
use crate::transaction_provider::{
    MockL1ProviderClient,
    ProposeTransactionProvider,
    ValidateTransactionProvider,
};

const INITIAL_HEIGHT: BlockNumber = BlockNumber(3);
const NEXT_HEIGHT: BlockNumber = BlockNumber(4);
const BLOCK_GENERATION_TIMEOUT: tokio::time::Duration = tokio::time::Duration::from_secs(1);
const MAX_L1_HANDLER_TXS_PER_BLOCK_PROPOSAL: usize = 3;
const INPUT_CHANNEL_SIZE: usize = 30;

#[fixture]
fn output_streaming() -> (
    tokio::sync::mpsc::UnboundedSender<Transaction>,
    tokio::sync::mpsc::UnboundedReceiver<Transaction>,
) {
    tokio::sync::mpsc::unbounded_channel()
}

struct MockDependencies {
    block_builder_factory: MockBlockBuilderFactoryTrait,
    storage_reader: MockBatcherStorageReaderTrait,
}

impl MockDependencies {
    fn expect_build_block(&mut self, times: usize) {
        let simulate_build_block = || -> BlockBuilderResult<Box<dyn BlockBuilderTrait>> {
            let mut mock_block_builder = MockBlockBuilderTrait::new();
            mock_block_builder
                .expect_build_block()
                .times(1)
                .return_once(move || Ok(BlockExecutionArtifacts::create_for_testing()));
            Ok(Box::new(mock_block_builder))
        };

        self.block_builder_factory
            .expect_create_block_builder()
            .times(times)
            .returning(move |_, _, _, _, _| simulate_build_block());
    }

    // This function simulates a long build block operation. This is required for a test that
    // tries to run other operations while a block is being built.
    fn expect_long_build_block(&mut self, times: usize) {
        let simulate_long_build_block = || -> BlockBuilderResult<Box<dyn BlockBuilderTrait>> {
            let mut mock_block_builder = MockBlockBuilderTrait::new();
            mock_block_builder.expect_build_block().times(1).return_once(move || {
                std::thread::sleep(BLOCK_GENERATION_TIMEOUT * 10);
                Ok(BlockExecutionArtifacts::create_for_testing())
            });
            Ok(Box::new(mock_block_builder))
        };

        self.block_builder_factory
            .expect_create_block_builder()
            .times(times)
            .returning(move |_, _, _, _, _| simulate_long_build_block());
    }

    fn expect_consequtive_storage_heights(&mut self) {
        self.storage_reader.checkpoint();
        self.storage_reader.expect_height().times(1).returning(|| Ok(INITIAL_HEIGHT));
        self.storage_reader.expect_height().times(1).returning(|| Ok(NEXT_HEIGHT));
    }
}

#[fixture]
fn mock_dependencies() -> MockDependencies {
    let mut storage_reader = MockBatcherStorageReaderTrait::new();
    storage_reader.expect_height().returning(|| Ok(INITIAL_HEIGHT));
    MockDependencies { block_builder_factory: MockBlockBuilderFactoryTrait::new(), storage_reader }
}

#[fixture]
fn propose_tx_provider() -> ProposeTransactionProvider {
    ProposeTransactionProvider::new(
        Arc::new(MockMempoolClient::new()),
        Arc::new(MockL1ProviderClient::new()),
        MAX_L1_HANDLER_TXS_PER_BLOCK_PROPOSAL,
    )
}

#[fixture]
fn validate_tx_provider() -> ValidateTransactionProvider {
    ValidateTransactionProvider {
        tx_receiver: tokio::sync::mpsc::channel(INPUT_CHANNEL_SIZE).1,
        l1_provider_client: Arc::new(MockL1ProviderClient::new()),
    }
}

fn proposal_manager(mock_dependencies: MockDependencies) -> ProposalManager {
    ProposalManager::new(
        Arc::new(mock_dependencies.block_builder_factory),
        Arc::new(mock_dependencies.storage_reader),
    )
}

fn proposal_deadline() -> tokio::time::Instant {
    tokio::time::Instant::now() + BLOCK_GENERATION_TIMEOUT
}

async fn build_proposal_non_blocking(
    proposal_manager: &mut ProposalManager,
    tx_provider: ProposeTransactionProvider,
    proposal_id: ProposalId,
) {
    let (output_sender, _receiver) = output_streaming();
    proposal_manager
        .build_block_proposal(proposal_id, None, proposal_deadline(), output_sender, tx_provider)
        .await
        .unwrap();
}

async fn build_proposal(
    proposal_manager: &mut ProposalManager,
    tx_provider: ProposeTransactionProvider,
    proposal_id: ProposalId,
) {
    build_proposal_non_blocking(proposal_manager, tx_provider, proposal_id).await;
    assert!(proposal_manager.await_active_proposal().await);
}

async fn validate_proposal(
    proposal_manager: &mut ProposalManager,
    tx_provider: ValidateTransactionProvider,
    proposal_id: ProposalId,
) {
    proposal_manager
        .validate_block_proposal(proposal_id, None, proposal_deadline(), tx_provider)
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
    let mut proposal_manager = proposal_manager(mock_dependencies);
    let result = proposal_manager.start_height(height).await;
    // Unfortunately ProposalManagerError doesn't implement PartialEq.
    assert_eq!(format!("{:?}", result), format!("{:?}", expected_result));
}

#[rstest]
#[tokio::test]
async fn duplicate_start_height(mock_dependencies: MockDependencies) {
    let mut proposal_manager = proposal_manager(mock_dependencies);
    assert_matches!(proposal_manager.start_height(INITIAL_HEIGHT).await, Ok(()));
    assert_matches!(
        proposal_manager.start_height(INITIAL_HEIGHT).await,
        Err(StartHeightError::HeightInProgress)
    );
}

#[rstest]
#[tokio::test]
async fn build_proposal_fails_without_start_height(
    mock_dependencies: MockDependencies,
    propose_tx_provider: ProposeTransactionProvider,
    output_streaming: (
        tokio::sync::mpsc::UnboundedSender<Transaction>,
        tokio::sync::mpsc::UnboundedReceiver<Transaction>,
    ),
) {
    let mut proposal_manager = proposal_manager(mock_dependencies);
    let err = proposal_manager
        .build_block_proposal(
            ProposalId(0),
            None,
            proposal_deadline(),
            output_streaming.0,
            propose_tx_provider,
        )
        .await;
    assert_matches!(err, Err(GenerateProposalError::NoActiveHeight));
}

#[rstest]
#[tokio::test]
async fn validate_proposal_fails_without_start_height(
    mock_dependencies: MockDependencies,
    validate_tx_provider: ValidateTransactionProvider,
) {
    let mut proposal_manager = proposal_manager(mock_dependencies);
    let err = proposal_manager
        .validate_block_proposal(ProposalId(0), None, proposal_deadline(), validate_tx_provider)
        .await;
    assert_matches!(err, Err(GenerateProposalError::NoActiveHeight));
}

#[rstest]
#[tokio::test]
async fn build_proposal_success(
    mut mock_dependencies: MockDependencies,
    propose_tx_provider: ProposeTransactionProvider,
) {
    mock_dependencies.expect_build_block(1);
    let mut proposal_manager = proposal_manager(mock_dependencies);

    proposal_manager.start_height(INITIAL_HEIGHT).await.unwrap();
    build_proposal(&mut proposal_manager, propose_tx_provider, ProposalId(0)).await;
    proposal_manager.take_proposal_result(ProposalId(0)).await.unwrap();
}

#[rstest]
#[tokio::test]
async fn validate_proposal_success(
    mut mock_dependencies: MockDependencies,
    validate_tx_provider: ValidateTransactionProvider,
) {
    mock_dependencies.expect_build_block(1);
    let mut proposal_manager = proposal_manager(mock_dependencies);

    proposal_manager.start_height(INITIAL_HEIGHT).await.unwrap();
    validate_proposal(&mut proposal_manager, validate_tx_provider, ProposalId(0)).await;
    proposal_manager.take_proposal_result(ProposalId(0)).await.unwrap();
}

#[rstest]
#[tokio::test]
async fn consecutive_proposal_generations_success(
    mut mock_dependencies: MockDependencies,
    propose_tx_provider: ProposeTransactionProvider,
) {
    mock_dependencies.expect_build_block(4);
    let mut proposal_manager = proposal_manager(mock_dependencies);

    proposal_manager.start_height(INITIAL_HEIGHT).await.unwrap();

    // Build and validate multiple proposals consecutively (awaiting on them to
    // make sure they finished successfully).
    build_proposal(&mut proposal_manager, propose_tx_provider.clone(), ProposalId(0)).await;
    validate_proposal(&mut proposal_manager, validate_tx_provider(), ProposalId(1)).await;
    build_proposal(&mut proposal_manager, propose_tx_provider, ProposalId(2)).await;
    validate_proposal(&mut proposal_manager, validate_tx_provider(), ProposalId(3)).await;
}

// This test checks that trying to generate a proposal while another one is being generated will
// fail. First the test will generate a new proposal that takes a very long time, and during
// that time it will send another build proposal request.
#[rstest]
#[tokio::test]
async fn multiple_proposals_generation_fail(
    mut mock_dependencies: MockDependencies,
    propose_tx_provider: ProposeTransactionProvider,
) {
    // Generate a block builder with a very long build block operation.
    mock_dependencies.expect_long_build_block(1);
    let mut proposal_manager = proposal_manager(mock_dependencies);

    proposal_manager.start_height(INITIAL_HEIGHT).await.unwrap();

    // Build a proposal that will take a very long time to finish.
    let (output_sender_0, _rec_0) = output_streaming();
    proposal_manager
        .build_block_proposal(
            ProposalId(0),
            None,
            proposal_deadline(),
            output_sender_0,
            propose_tx_provider.clone(),
        )
        .await
        .unwrap();

    // Try to generate another proposal while the first one is still being generated.
    let (output_sender_1, _rec_1) = output_streaming();
    let another_generate_request = proposal_manager
        .build_block_proposal(
            ProposalId(1),
            None,
            proposal_deadline(),
            output_sender_1,
            propose_tx_provider,
        )
        .await;
    assert_matches!(
        another_generate_request,
        Err(GenerateProposalError::AlreadyGeneratingProposal {
            current_generating_proposal_id,
            new_proposal_id
        }) if current_generating_proposal_id == ProposalId(0) && new_proposal_id == ProposalId(1)
    );
}

#[rstest]
#[tokio::test]
async fn take_proposal_result_no_active_proposal(
    mut mock_dependencies: MockDependencies,
    propose_tx_provider: ProposeTransactionProvider,
) {
    mock_dependencies.expect_build_block(1);
    let mut proposal_manager = proposal_manager(mock_dependencies);

    proposal_manager.start_height(INITIAL_HEIGHT).await.unwrap();

    build_proposal(&mut proposal_manager, propose_tx_provider, ProposalId(0)).await;

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
async fn abort_active_proposal(
    mut mock_dependencies: MockDependencies,
    propose_tx_provider: ProposeTransactionProvider,
) {
    mock_dependencies.expect_long_build_block(1);
    let mut proposal_manager = proposal_manager(mock_dependencies);

    proposal_manager.start_height(INITIAL_HEIGHT).await.unwrap();
    build_proposal_non_blocking(&mut proposal_manager, propose_tx_provider, ProposalId(0)).await;

    proposal_manager.abort_proposal(ProposalId(0)).await;

    assert_matches!(
        proposal_manager.take_proposal_result(ProposalId(0)).await,
        Err(GetProposalResultError::Aborted)
    );

    // Make sure there is no active proposal.
    assert!(!proposal_manager.await_active_proposal().await);
}

#[rstest]
#[tokio::test]
async fn abort_and_start_new_height(
    mut mock_dependencies: MockDependencies,
    propose_tx_provider: ProposeTransactionProvider,
) {
    mock_dependencies.expect_consequtive_storage_heights();

    mock_dependencies.expect_build_block(1);
    mock_dependencies.expect_long_build_block(1);
    let mut proposal_manager = proposal_manager(mock_dependencies);

    // Start the first height with 2 proposals.
    proposal_manager.start_height(INITIAL_HEIGHT).await.unwrap();
    build_proposal(&mut proposal_manager, propose_tx_provider.clone(), ProposalId(0)).await;
    build_proposal_non_blocking(&mut proposal_manager, propose_tx_provider.clone(), ProposalId(1))
        .await;

    // Start a new height. This should abort and delete all existing proposals.
    assert!(proposal_manager.start_height(NEXT_HEIGHT).await.is_ok());

    // Make sure executed proposals are deleted.
    assert_matches!(
        proposal_manager.take_proposal_result(ProposalId(0)).await,
        Err(GetProposalResultError::ProposalDoesNotExist { .. })
    );

    // Make sure there is no active proposal.
    assert!(!proposal_manager.await_active_proposal().await);
}
