use assert_matches::assert_matches;
use rstest::{fixture, rstest};
use starknet_api::executable_transaction::Transaction;
use starknet_batcher_types::batcher_types::ProposalId;

use crate::block_builder::{BlockBuilderTrait, BlockExecutionArtifacts, MockBlockBuilderTrait};
use crate::proposal_manager::{
    GenerateProposalError,
    GetProposalResultError,
    ProposalManager,
    ProposalManagerTrait,
    ProposalOutput,
};

const BLOCK_GENERATION_TIMEOUT: tokio::time::Duration = tokio::time::Duration::from_secs(1);

#[fixture]
fn output_streaming() -> (
    tokio::sync::mpsc::UnboundedSender<Transaction>,
    tokio::sync::mpsc::UnboundedReceiver<Transaction>,
) {
    tokio::sync::mpsc::unbounded_channel()
}

#[fixture]
fn proposal_manager() -> ProposalManager {
    ProposalManager::new()
}

fn mock_build_block() -> Box<MockBlockBuilderTrait> {
    let mut mock_block_builder = MockBlockBuilderTrait::new();
    mock_block_builder
        .expect_build_block()
        .times(1)
        .return_once(move || Ok(BlockExecutionArtifacts::create_for_testing()));
    Box::new(mock_block_builder)
}

// This function simulates a long build block operation. This is required for a test that
// tries to run other operations while a block is being built.
fn mock_long_build_block() -> Box<MockBlockBuilderTrait> {
    let mut mock_block_builder = MockBlockBuilderTrait::new();
    mock_block_builder.expect_build_block().times(1).return_once(move || {
        std::thread::sleep(BLOCK_GENERATION_TIMEOUT * 10);
        Ok(BlockExecutionArtifacts::create_for_testing())
    });
    Box::new(mock_block_builder)
}

async fn spawn_proposal_non_blocking(
    proposal_manager: &mut ProposalManager,
    proposal_id: ProposalId,
    block_builder: Box<dyn BlockBuilderTrait>,
) -> Result<(), GenerateProposalError> {
    let (abort_sender, _rec) = tokio::sync::oneshot::channel();
    proposal_manager.spawn_proposal(proposal_id, block_builder, abort_sender).await
}

async fn spawn_proposal(
    proposal_manager: &mut ProposalManager,
    proposal_id: ProposalId,
    block_builder: Box<dyn BlockBuilderTrait>,
) {
    spawn_proposal_non_blocking(proposal_manager, proposal_id, block_builder).await.unwrap();
    assert!(proposal_manager.await_active_proposal().await);
}

#[rstest]
#[tokio::test]
async fn spawn_proposal_success(mut proposal_manager: ProposalManager) {
    spawn_proposal(&mut proposal_manager, ProposalId(0), mock_build_block()).await;

    proposal_manager.take_proposal_result(ProposalId(0)).await.unwrap();
}

#[rstest]
#[tokio::test]
async fn consecutive_proposal_generations_success(mut proposal_manager: ProposalManager) {
    // Build and validate multiple proposals consecutively (awaiting on them to
    // make sure they finished successfully).
    spawn_proposal(&mut proposal_manager, ProposalId(0), mock_build_block()).await;
    spawn_proposal(&mut proposal_manager, ProposalId(1), mock_build_block()).await;
}

// This test checks that trying to generate a proposal while another one is being generated will
// fail. First the test will generate a new proposal that takes a very long time, and during
// that time it will send another build proposal request.
#[rstest]
#[tokio::test]
async fn multiple_proposals_generation_fail(mut proposal_manager: ProposalManager) {
    // Build a proposal that will take a very long time to finish.
    spawn_proposal_non_blocking(&mut proposal_manager, ProposalId(0), mock_long_build_block())
        .await
        .unwrap();

    // Try to generate another proposal while the first one is still being generated.
    let mut block_builder = MockBlockBuilderTrait::new();
    block_builder.expect_build_block().never();
    let another_generate_request =
        spawn_proposal_non_blocking(&mut proposal_manager, ProposalId(1), Box::new(block_builder))
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
async fn take_proposal_result_no_active_proposal(mut proposal_manager: ProposalManager) {
    spawn_proposal(&mut proposal_manager, ProposalId(0), mock_build_block()).await;

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
async fn abort_active_proposal(mut proposal_manager: ProposalManager) {
    spawn_proposal_non_blocking(&mut proposal_manager, ProposalId(0), mock_long_build_block())
        .await
        .unwrap();

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
async fn reset(mut proposal_manager: ProposalManager) {
    // Create 2 proposals, one will remain active.
    spawn_proposal(&mut proposal_manager, ProposalId(0), mock_build_block()).await;
    spawn_proposal_non_blocking(&mut proposal_manager, ProposalId(1), mock_long_build_block())
        .await
        .unwrap();

    proposal_manager.reset().await;

    // Make sure executed proposals are deleted.
    assert_matches!(
        proposal_manager.take_proposal_result(ProposalId(0)).await,
        Err(GetProposalResultError::ProposalDoesNotExist { .. })
    );

    // Make sure there is no active proposal.
    assert!(!proposal_manager.await_active_proposal().await);
}
