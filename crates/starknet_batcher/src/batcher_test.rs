use std::sync::Arc;

use assert_matches::assert_matches;
use blockifier::abi::constants;
use blockifier::test_utils::struct_impls::BlockInfoExt;
use chrono::Utc;
use mockall::predicate::eq;
use rstest::rstest;
use starknet_api::block::{BlockInfo, BlockNumber};
use starknet_api::executable_transaction::Transaction;
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

use crate::batcher::{
    Batcher,
    MockBatcherStorageReaderTrait,
    MockBatcherStorageWriterTrait,
    ProposalOutput,
};
use crate::block_builder::{
    AbortSignalSender,
    BlockBuilderError,
    BlockBuilderResult,
    BlockExecutionArtifacts,
    FailOnErrorCause,
    MockBlockBuilderFactoryTrait,
};
use crate::config::BatcherConfig;
use crate::test_utils::{test_txs, FakeProposeBlockBuilder, FakeValidateBlockBuilder};

const INITIAL_HEIGHT: BlockNumber = BlockNumber(3);
const STREAMING_CHUNK_SIZE: usize = 3;
const BLOCK_GENERATION_TIMEOUT: tokio::time::Duration = tokio::time::Duration::from_secs(1);
const PROPOSAL_ID: ProposalId = ProposalId(0);

fn initial_block_info() -> BlockInfo {
    BlockInfo { block_number: INITIAL_HEIGHT, ..BlockInfo::create_for_testing() }
}

fn proposal_commitment() -> ProposalCommitment {
    ProposalOutput::from(BlockExecutionArtifacts::create_for_testing()).commitment
}

fn deadline() -> chrono::DateTime<Utc> {
    chrono::Utc::now() + BLOCK_GENERATION_TIMEOUT
}

struct MockDependencies {
    storage_reader: MockBatcherStorageReaderTrait,
    storage_writer: MockBatcherStorageWriterTrait,
    mempool_client: MockMempoolClient,
    block_builder_factory: MockBlockBuilderFactoryTrait,
}

impl Default for MockDependencies {
    fn default() -> Self {
        let mut storage_reader = MockBatcherStorageReaderTrait::new();
        storage_reader.expect_height().returning(|| Ok(INITIAL_HEIGHT));
        Self {
            storage_reader,
            storage_writer: MockBatcherStorageWriterTrait::new(),
            mempool_client: MockMempoolClient::new(),
            block_builder_factory: MockBlockBuilderFactoryTrait::new(),
        }
    }
}

fn create_batcher(mock_dependencies: MockDependencies) -> Batcher {
    Batcher::new(
        BatcherConfig { outstream_content_buffer_size: STREAMING_CHUNK_SIZE, ..Default::default() },
        Arc::new(mock_dependencies.storage_reader),
        Box::new(mock_dependencies.storage_writer),
        Arc::new(mock_dependencies.mempool_client),
        Box::new(mock_dependencies.block_builder_factory),
    )
}

fn abort_signal_sender() -> AbortSignalSender {
    tokio::sync::oneshot::channel().0
}

fn expect_create_validate_block_builder(
    block_builder_factory: &mut MockBlockBuilderFactoryTrait,
    build_block_result: BlockBuilderResult<BlockExecutionArtifacts>,
) {
    block_builder_factory.expect_create_block_builder().times(1).return_once(
        |_, _, tx_provider, _| {
            Ok((
                Box::new(FakeValidateBlockBuilder { tx_provider, build_block_result }),
                abort_signal_sender(),
            ))
        },
    );
}

fn expect_create_propose_block_builder(
    block_builder_factory: &mut MockBlockBuilderFactoryTrait,
    output_txs: Vec<Transaction>,
    build_block_result: BlockBuilderResult<BlockExecutionArtifacts>,
) {
    block_builder_factory.expect_create_block_builder().times(1).return_once(
        move |_, _, _, output_content_sender| {
            Ok((
                Box::new(FakeProposeBlockBuilder {
                    output_content_sender: output_content_sender.unwrap(),
                    output_txs,
                    build_block_result,
                }),
                abort_signal_sender(),
            ))
        },
    );
}

fn successful_validate_block_builder() -> MockBlockBuilderFactoryTrait {
    let mut block_builder_factory = MockBlockBuilderFactoryTrait::new();
    expect_create_validate_block_builder(
        &mut block_builder_factory,
        Ok(BlockExecutionArtifacts::create_for_testing()),
    );
    block_builder_factory
}

fn failed_validate_block_builder() -> MockBlockBuilderFactoryTrait {
    let mut block_builder_factory = MockBlockBuilderFactoryTrait::new();
    expect_create_validate_block_builder(
        &mut block_builder_factory,
        Err(BlockBuilderError::FailOnError(FailOnErrorCause::BlockFull)),
    );
    block_builder_factory
}

fn successful_propose_block_builder(output_txs: Vec<Transaction>) -> MockBlockBuilderFactoryTrait {
    let mut block_builder_factory = MockBlockBuilderFactoryTrait::new();
    expect_create_propose_block_builder(
        &mut block_builder_factory,
        output_txs,
        Ok(BlockExecutionArtifacts::create_for_testing()),
    );
    block_builder_factory
}

async fn create_completed_validate_proposal(batcher: &mut Batcher) {
    create_active_validate_proposal(batcher).await;

    let finish_proposal_input =
        SendProposalContentInput { proposal_id: PROPOSAL_ID, content: SendProposalContent::Finish };
    batcher.send_proposal_content(finish_proposal_input).await.unwrap();

    // Make sure the proposal is finished.
    batcher.await_active_proposal().await;
}

async fn create_active_validate_proposal(batcher: &mut Batcher) {
    batcher.start_height(StartHeightInput { height: INITIAL_HEIGHT }).await.unwrap();

    let validate_block_input = ValidateBlockInput {
        proposal_id: PROPOSAL_ID,
        deadline: deadline(),
        retrospective_block_hash: None,
        block_info: initial_block_info(),
    };
    batcher.validate_block(validate_block_input).await.unwrap();
}

#[rstest]
#[tokio::test]
async fn start_height_success() {
    let mut batcher = create_batcher(MockDependencies::default());
    assert_eq!(batcher.start_height(StartHeightInput { height: INITIAL_HEIGHT }).await, Ok(()));
}

#[rstest]
#[case::height_already_passed(
    INITIAL_HEIGHT.prev().unwrap(),
    BatcherError::HeightAlreadyPassed {
        storage_height: INITIAL_HEIGHT,
        requested_height: INITIAL_HEIGHT.prev().unwrap()
    }
)]
#[case::storage_not_synced(
    INITIAL_HEIGHT.unchecked_next(),
    BatcherError::StorageNotSynced {
        storage_height: INITIAL_HEIGHT,
        requested_height: INITIAL_HEIGHT.unchecked_next()
    }
)]
#[tokio::test]
async fn start_height_fail(#[case] height: BlockNumber, #[case] expected_error: BatcherError) {
    let mut batcher = create_batcher(MockDependencies::default());
    assert_eq!(batcher.start_height(StartHeightInput { height }).await, Err(expected_error));
}

#[rstest]
#[tokio::test]
async fn duplicate_start_height() {
    let mut batcher = create_batcher(MockDependencies::default());

    let initial_height = StartHeightInput { height: INITIAL_HEIGHT };
    assert_eq!(batcher.start_height(initial_height.clone()).await, Ok(()));
    assert_eq!(batcher.start_height(initial_height).await, Err(BatcherError::HeightInProgress));
}

#[rstest]
#[tokio::test]
async fn no_active_height() {
    let mut batcher = create_batcher(MockDependencies::default());

    // Calling `propose_block` and `validate_block` without starting a height should fail.

    let result = batcher
        .propose_block(ProposeBlockInput {
            proposal_id: ProposalId(0),
            retrospective_block_hash: None,
            deadline: chrono::Utc::now() + chrono::Duration::seconds(1),
            block_info: Default::default(),
        })
        .await;
    assert_eq!(result, Err(BatcherError::NoActiveHeight));

    let result = batcher
        .validate_block(ValidateBlockInput {
            proposal_id: ProposalId(0),
            retrospective_block_hash: None,
            deadline: chrono::Utc::now() + chrono::Duration::seconds(1),
            block_info: Default::default(),
        })
        .await;
    assert_eq!(result, Err(BatcherError::NoActiveHeight));
}

#[rstest]
#[tokio::test]
async fn consecutive_proposal_generation_success() {
    let mut block_builder_factory = MockBlockBuilderFactoryTrait::new();
    expect_create_validate_block_builder(
        &mut block_builder_factory,
        Ok(BlockExecutionArtifacts::create_for_testing()),
    );
    expect_create_propose_block_builder(
        &mut block_builder_factory,
        vec![],
        Ok(BlockExecutionArtifacts::create_for_testing()),
    );
    let mut batcher =
        create_batcher(MockDependencies { block_builder_factory, ..Default::default() });

    create_completed_validate_proposal(&mut batcher).await;

    // Make sure another proposal can be generated after the first one finished.
    batcher
        .propose_block(ProposeBlockInput {
            proposal_id: ProposalId(1),
            retrospective_block_hash: None,
            deadline: chrono::Utc::now() + chrono::Duration::seconds(1),
            block_info: initial_block_info(),
        })
        .await
        .unwrap();
}

#[rstest]
#[tokio::test]
async fn concurrent_proposals_generation_fail() {
    let mut batcher = create_batcher(MockDependencies {
        block_builder_factory: successful_validate_block_builder(),
        ..Default::default()
    });

    // Start a validate proposal that will remain active.
    create_active_validate_proposal(&mut batcher).await;

    // Make sure another proposal can't be generated while the first one is still active.
    let result = batcher
        .propose_block(ProposeBlockInput {
            proposal_id: ProposalId(1),
            retrospective_block_hash: None,
            deadline: chrono::Utc::now() + chrono::Duration::seconds(1),
            block_info: initial_block_info(),
        })
        .await;

    assert_matches!(result, Err(BatcherError::ServerBusy { .. }));
}

#[rstest]
#[tokio::test]
async fn validate_block_full_flow() {
    let mut batcher = create_batcher(MockDependencies {
        block_builder_factory: successful_validate_block_builder(),
        ..Default::default()
    });
    create_active_validate_proposal(&mut batcher).await;

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
async fn send_proposal_content_abort() {
    let mut batcher = create_batcher(MockDependencies {
        block_builder_factory: successful_validate_block_builder(),
        ..Default::default()
    });
    create_active_validate_proposal(&mut batcher).await;

    let send_abort_request =
        SendProposalContentInput { proposal_id: PROPOSAL_ID, content: SendProposalContent::Abort };
    assert_eq!(
        batcher.send_proposal_content(send_abort_request).await.unwrap(),
        SendProposalContentResponse { response: ProposalStatus::Aborted }
    );
}

#[rstest]
#[tokio::test]
async fn send_content_after_proposal_already_finished() {
    let mut batcher = create_batcher(MockDependencies {
        block_builder_factory: successful_validate_block_builder(),
        ..Default::default()
    });
    create_completed_validate_proposal(&mut batcher).await;

    // Send transactions after the proposal has finished.
    let send_proposal_input_txs = SendProposalContentInput {
        proposal_id: PROPOSAL_ID,
        content: SendProposalContent::Txs(test_txs(0..1)),
    };
    let result = batcher.send_proposal_content(send_proposal_input_txs).await;
    assert_eq!(result, Err(BatcherError::ProposalNotFound { proposal_id: PROPOSAL_ID }));
}

#[rstest]
#[tokio::test]
async fn send_content_to_unknown_proposal() {
    let mut batcher = create_batcher(MockDependencies::default());

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
    let mut batcher = create_batcher(MockDependencies {
        block_builder_factory: failed_validate_block_builder(),
        ..Default::default()
    });
    create_active_validate_proposal(&mut batcher).await;
    batcher.await_active_proposal().await;

    let send_proposal_input_txs = SendProposalContentInput {
        proposal_id: PROPOSAL_ID,
        content: SendProposalContent::Txs(test_txs(0..1)),
    };
    let result = batcher.send_proposal_content(send_proposal_input_txs).await.unwrap();
    assert_eq!(result, SendProposalContentResponse { response: ProposalStatus::InvalidProposal });
}

#[rstest]
#[tokio::test]
async fn propose_block_full_flow() {
    // Expecting 3 chunks of streamed txs.
    let expected_streamed_txs = test_txs(0..STREAMING_CHUNK_SIZE * 2 + 1);

    let mut batcher = create_batcher(MockDependencies {
        block_builder_factory: successful_propose_block_builder(expected_streamed_txs.clone()),
        ..Default::default()
    });

    batcher.start_height(StartHeightInput { height: INITIAL_HEIGHT }).await.unwrap();
    batcher
        .propose_block(ProposeBlockInput {
            proposal_id: PROPOSAL_ID,
            retrospective_block_hash: None,
            deadline: chrono::Utc::now() + chrono::Duration::seconds(1),
            block_info: initial_block_info(),
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
    assert_eq!(
        commitment,
        GetProposalContentResponse { content: GetProposalContent::Finished(proposal_commitment()) }
    );

    let exhausted =
        batcher.get_proposal_content(GetProposalContentInput { proposal_id: PROPOSAL_ID }).await;
    assert_matches!(exhausted, Err(BatcherError::ProposalNotFound { .. }));
}

#[rstest]
#[tokio::test]
async fn propose_block_without_retrospective_block_hash() {
    let mut storage_reader = MockBatcherStorageReaderTrait::new();
    storage_reader
        .expect_height()
        .returning(|| Ok(BlockNumber(constants::STORED_BLOCK_HASH_BUFFER)));

    let mut batcher = create_batcher(MockDependencies { storage_reader, ..Default::default() });

    batcher
        .start_height(StartHeightInput { height: BlockNumber(constants::STORED_BLOCK_HASH_BUFFER) })
        .await
        .unwrap();
    let result = batcher
        .propose_block(ProposeBlockInput {
            proposal_id: PROPOSAL_ID,
            retrospective_block_hash: None,
            deadline: deadline(),
            block_info: Default::default(),
        })
        .await;

    assert_matches!(result, Err(BatcherError::MissingRetrospectiveBlockHash));
}

#[rstest]
#[tokio::test]
async fn get_content_from_unknown_proposal() {
    let mut batcher = create_batcher(MockDependencies::default());

    let get_proposal_content_input = GetProposalContentInput { proposal_id: PROPOSAL_ID };
    let result = batcher.get_proposal_content(get_proposal_content_input).await;
    assert_eq!(result, Err(BatcherError::ProposalNotFound { proposal_id: PROPOSAL_ID }));
}

#[rstest]
#[tokio::test]
async fn decision_reached() {
    let mut mock_dependencies = MockDependencies::default();
    let expected_proposal_output =
        ProposalOutput::from(BlockExecutionArtifacts::create_for_testing());

    mock_dependencies
        .mempool_client
        .expect_commit_block()
        .with(eq(CommitBlockArgs {
            address_to_nonce: expected_proposal_output.nonces,
            tx_hashes: expected_proposal_output.tx_hashes,
        }))
        .returning(|_| Ok(()));

    mock_dependencies
        .storage_writer
        .expect_commit_proposal()
        .with(eq(INITIAL_HEIGHT), eq(expected_proposal_output.state_diff))
        .returning(|_, _| Ok(()));

    mock_dependencies.block_builder_factory = successful_validate_block_builder();
    let mut batcher = create_batcher(mock_dependencies);

    create_completed_validate_proposal(&mut batcher).await;

    batcher.decision_reached(DecisionReachedInput { proposal_id: PROPOSAL_ID }).await.unwrap();
}

#[rstest]
#[tokio::test]
async fn decision_reached_no_executed_proposal() {
    let expected_error = BatcherError::ExecutedProposalNotFound { proposal_id: PROPOSAL_ID };

    let mut batcher = create_batcher(MockDependencies::default());
    batcher.start_height(StartHeightInput { height: INITIAL_HEIGHT }).await.unwrap();

    let decision_reached_result =
        batcher.decision_reached(DecisionReachedInput { proposal_id: PROPOSAL_ID }).await;
    assert_eq!(decision_reached_result, Err(expected_error));
}
