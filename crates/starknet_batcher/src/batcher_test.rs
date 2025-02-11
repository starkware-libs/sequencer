use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use assert_matches::assert_matches;
use blockifier::abi::constants;
use blockifier::transaction::objects::TransactionExecutionInfo;
use indexmap::indexmap;
use metrics_exporter_prometheus::PrometheusBuilder;
use mockall::predicate::eq;
use rstest::rstest;
use starknet_api::block::{BlockHeaderWithoutHash, BlockInfo, BlockNumber};
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::core::{ChainId, ContractAddress, Nonce};
use starknet_api::state::ThinStateDiff;
use starknet_api::transaction::TransactionHash;
use starknet_api::{contract_address, nonce, tx_hash};
use starknet_batcher_types::batcher_types::{
    DecisionReachedInput,
    DecisionReachedResponse,
    GetHeightResponse,
    GetProposalContent,
    GetProposalContentInput,
    GetProposalContentResponse,
    ProposalCommitment,
    ProposalId,
    ProposalStatus,
    ProposeBlockInput,
    RevertBlockInput,
    SendProposalContent,
    SendProposalContentInput,
    SendProposalContentResponse,
    StartHeightInput,
    ValidateBlockInput,
};
use starknet_batcher_types::errors::BatcherError;
use starknet_class_manager_types::transaction_converter::TransactionConverter;
use starknet_class_manager_types::{EmptyClassManagerClient, SharedClassManagerClient};
use starknet_l1_provider_types::errors::L1ProviderError;
use starknet_l1_provider_types::{MockL1ProviderClient, SessionState};
use starknet_mempool_types::communication::{MempoolClientError, MockMempoolClient};
use starknet_mempool_types::mempool_types::CommitBlockArgs;
use starknet_sequencer_infra::component_client::ClientError;
use starknet_sequencer_infra::component_definitions::ComponentStarter;
use starknet_sequencer_metrics::metric_definitions::{
    BATCHED_TRANSACTIONS,
    PROPOSAL_ABORTED,
    PROPOSAL_FAILED,
    PROPOSAL_STARTED,
    PROPOSAL_SUCCEEDED,
    REJECTED_TRANSACTIONS,
    STORAGE_HEIGHT,
};
use starknet_state_sync_types::state_sync_types::SyncBlock;

use crate::batcher::{Batcher, MockBatcherStorageReaderTrait, MockBatcherStorageWriterTrait};
use crate::block_builder::{
    AbortSignalSender,
    BlockBuilderError,
    BlockBuilderResult,
    BlockExecutionArtifacts,
    FailOnErrorCause,
    MockBlockBuilderFactoryTrait,
};
use crate::config::BatcherConfig;
use crate::test_utils::{
    test_txs,
    verify_indexed_execution_infos,
    FakeProposeBlockBuilder,
    FakeValidateBlockBuilder,
};

const INITIAL_HEIGHT: BlockNumber = BlockNumber(3);
const LATEST_BLOCK_IN_STORAGE: BlockNumber = BlockNumber(INITIAL_HEIGHT.0 - 1);
const STREAMING_CHUNK_SIZE: usize = 3;
const BLOCK_GENERATION_TIMEOUT: tokio::time::Duration = tokio::time::Duration::from_secs(1);
const PROPOSAL_ID: ProposalId = ProposalId(0);
const BUILD_BLOCK_FAIL_ON_ERROR: BlockBuilderError =
    BlockBuilderError::FailOnError(FailOnErrorCause::BlockFull);

fn proposal_commitment() -> ProposalCommitment {
    BlockExecutionArtifacts::create_for_testing().commitment()
}

fn propose_block_input(proposal_id: ProposalId) -> ProposeBlockInput {
    ProposeBlockInput {
        proposal_id,
        retrospective_block_hash: None,
        deadline: chrono::Utc::now() + BLOCK_GENERATION_TIMEOUT,
        block_info: BlockInfo { block_number: INITIAL_HEIGHT, ..BlockInfo::create_for_testing() },
    }
}

fn validate_block_input(proposal_id: ProposalId) -> ValidateBlockInput {
    ValidateBlockInput {
        proposal_id,
        retrospective_block_hash: None,
        deadline: chrono::Utc::now() + BLOCK_GENERATION_TIMEOUT,
        block_info: BlockInfo { block_number: INITIAL_HEIGHT, ..BlockInfo::create_for_testing() },
    }
}

struct MockDependencies {
    storage_reader: MockBatcherStorageReaderTrait,
    storage_writer: MockBatcherStorageWriterTrait,
    mempool_client: MockMempoolClient,
    l1_provider_client: MockL1ProviderClient,
    block_builder_factory: MockBlockBuilderFactoryTrait,
    class_manager_client: SharedClassManagerClient,
}

impl Default for MockDependencies {
    fn default() -> Self {
        let mut storage_reader = MockBatcherStorageReaderTrait::new();
        storage_reader.expect_height().returning(|| Ok(INITIAL_HEIGHT));
        let mut mempool_client = MockMempoolClient::new();
        let expected_gas_price =
            propose_block_input(PROPOSAL_ID).block_info.gas_prices.strk_gas_prices.l2_gas_price;
        mempool_client.expect_update_gas_price().with(eq(expected_gas_price)).returning(|_| Ok(()));
        Self {
            storage_reader,
            storage_writer: MockBatcherStorageWriterTrait::new(),
            l1_provider_client: MockL1ProviderClient::new(),
            mempool_client,
            block_builder_factory: MockBlockBuilderFactoryTrait::new(),
            // TODO(noamsp): use MockClassManagerClient
            class_manager_client: Arc::new(EmptyClassManagerClient),
        }
    }
}

async fn create_batcher(mock_dependencies: MockDependencies) -> Batcher {
    let mut batcher = Batcher::new(
        BatcherConfig { outstream_content_buffer_size: STREAMING_CHUNK_SIZE, ..Default::default() },
        Arc::new(mock_dependencies.storage_reader),
        Box::new(mock_dependencies.storage_writer),
        Arc::new(mock_dependencies.l1_provider_client),
        Arc::new(mock_dependencies.mempool_client),
        TransactionConverter::new(
            mock_dependencies.class_manager_client,
            ChainId::create_for_testing(),
        ),
        Box::new(mock_dependencies.block_builder_factory),
    );
    // Call post-creation functionality (e.g., metrics registration).
    batcher.start().await.unwrap();
    batcher
}

fn abort_signal_sender() -> AbortSignalSender {
    tokio::sync::oneshot::channel().0
}

async fn batcher_propose_and_commit_block(
    mock_dependencies: MockDependencies,
) -> DecisionReachedResponse {
    let mut batcher = create_batcher(mock_dependencies).await;
    batcher.start_height(StartHeightInput { height: INITIAL_HEIGHT }).await.unwrap();
    batcher.propose_block(propose_block_input(PROPOSAL_ID)).await.unwrap();
    batcher.await_active_proposal().await;
    batcher.decision_reached(DecisionReachedInput { proposal_id: PROPOSAL_ID }).await.unwrap()
}

fn mock_create_builder_for_validate_block(
    block_builder_factory: &mut MockBlockBuilderFactoryTrait,
    build_block_result: BlockBuilderResult<BlockExecutionArtifacts>,
) {
    block_builder_factory.expect_create_block_builder().times(1).return_once(
        |_, _, tx_provider, _| {
            let block_builder = FakeValidateBlockBuilder {
                tx_provider,
                build_block_result: Some(build_block_result),
            };
            Ok((Box::new(block_builder), abort_signal_sender()))
        },
    );
}

fn mock_create_builder_for_propose_block(
    block_builder_factory: &mut MockBlockBuilderFactoryTrait,
    output_txs: Vec<InternalConsensusTransaction>,
    build_block_result: BlockBuilderResult<BlockExecutionArtifacts>,
) {
    block_builder_factory.expect_create_block_builder().times(1).return_once(
        move |_, _, _, output_content_sender| {
            let block_builder = FakeProposeBlockBuilder {
                output_content_sender: output_content_sender.unwrap(),
                output_txs,
                build_block_result: Some(build_block_result),
            };
            Ok((Box::new(block_builder), abort_signal_sender()))
        },
    );
}

async fn batcher_with_active_validate_block(
    build_block_result: BlockBuilderResult<BlockExecutionArtifacts>,
) -> Batcher {
    let mut block_builder_factory = MockBlockBuilderFactoryTrait::new();
    mock_create_builder_for_validate_block(&mut block_builder_factory, build_block_result);

    let mut l1_provider_client = MockL1ProviderClient::new();
    l1_provider_client.expect_start_block().returning(|_, _| Ok(()));

    let mut batcher = create_batcher(MockDependencies {
        block_builder_factory,
        l1_provider_client,
        ..Default::default()
    })
    .await;

    batcher.start_height(StartHeightInput { height: INITIAL_HEIGHT }).await.unwrap();

    batcher.validate_block(validate_block_input(PROPOSAL_ID)).await.unwrap();

    batcher
}

fn test_tx_hashes() -> HashSet<TransactionHash> {
    (0..5u8).map(|i| tx_hash!(i + 12)).collect()
}

fn test_contract_nonces() -> HashMap<ContractAddress, Nonce> {
    HashMap::from_iter((0..3u8).map(|i| (contract_address!(i + 33), nonce!(i + 9))))
}

pub fn test_state_diff() -> ThinStateDiff {
    ThinStateDiff {
        storage_diffs: indexmap! {
            4u64.into() => indexmap! {
                5u64.into() => 6u64.into(),
                7u64.into() => 8u64.into(),
            },
            9u64.into() => indexmap! {
                10u64.into() => 11u64.into(),
            },
        },
        nonces: test_contract_nonces().into_iter().collect(),
        ..Default::default()
    }
}

fn verify_decision_reached_response(
    response: &DecisionReachedResponse,
    expected_artifacts: &BlockExecutionArtifacts,
) {
    assert_eq!(
        response.state_diff.nonces,
        expected_artifacts.commitment_state_diff.address_to_nonce
    );
    assert_eq!(
        response.state_diff.storage_diffs,
        expected_artifacts.commitment_state_diff.storage_updates
    );
    assert_eq!(
        response.state_diff.declared_classes,
        expected_artifacts.commitment_state_diff.class_hash_to_compiled_class_hash
    );
    assert_eq!(
        response.state_diff.deployed_contracts,
        expected_artifacts.commitment_state_diff.address_to_class_hash
    );
    assert_eq!(response.l2_gas_used, expected_artifacts.l2_gas_used);
    assert_eq!(response.central_objects.bouncer_weights, expected_artifacts.bouncer_weights);
    assert_eq!(
        response.central_objects.execution_infos,
        expected_artifacts.execution_infos.values().cloned().collect::<Vec<_>>()
    );
}

fn assert_proposal_metrics(
    metrics: &str,
    expected_started: u64,
    expected_succeeded: u64,
    expected_failed: u64,
    expected_aborted: u64,
) {
    let n_expected_active_proposals =
        expected_started - (expected_succeeded + expected_failed + expected_aborted);
    assert!(n_expected_active_proposals <= 1);
    let started = PROPOSAL_STARTED.parse_numeric_metric::<u64>(metrics);
    let succeeded = PROPOSAL_SUCCEEDED.parse_numeric_metric::<u64>(metrics);
    let failed = PROPOSAL_FAILED.parse_numeric_metric::<u64>(metrics);
    let aborted = PROPOSAL_ABORTED.parse_numeric_metric::<u64>(metrics);

    assert_eq!(
        started,
        Some(expected_started),
        "unexpected value proposal_started, expected {} got {:?}",
        expected_started,
        started,
    );
    assert_eq!(
        succeeded,
        Some(expected_succeeded),
        "unexpected value proposal_succeeded, expected {} got {:?}",
        expected_succeeded,
        succeeded,
    );
    assert_eq!(
        failed,
        Some(expected_failed),
        "unexpected value proposal_failed, expected {} got {:?}",
        expected_failed,
        failed,
    );
    assert_eq!(
        aborted,
        Some(expected_aborted),
        "unexpected value proposal_aborted, expected {} got {:?}",
        expected_aborted,
        aborted,
    );
}

#[tokio::test]
async fn metrics_registered() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    let _batcher = create_batcher(MockDependencies::default()).await;
    let metrics = recorder.handle().render();
    assert_eq!(STORAGE_HEIGHT.parse_numeric_metric::<u64>(&metrics), Some(INITIAL_HEIGHT.0));
}

#[rstest]
#[tokio::test]
async fn start_height_success() {
    let mut batcher = create_batcher(MockDependencies::default()).await;
    assert_eq!(batcher.start_height(StartHeightInput { height: INITIAL_HEIGHT }).await, Ok(()));
}

#[rstest]
#[case::height_already_passed(
    INITIAL_HEIGHT.prev().unwrap(),
    BatcherError::StorageHeightMarkerMismatch {
        marker_height: INITIAL_HEIGHT,
        requested_height: INITIAL_HEIGHT.prev().unwrap()
    }
)]
#[case::storage_not_synced(
    INITIAL_HEIGHT.unchecked_next(),
    BatcherError::StorageHeightMarkerMismatch {
        marker_height: INITIAL_HEIGHT,
        requested_height: INITIAL_HEIGHT.unchecked_next()
    }
)]
#[tokio::test]
async fn start_height_fail(#[case] height: BlockNumber, #[case] expected_error: BatcherError) {
    let mut batcher = create_batcher(MockDependencies::default()).await;
    assert_eq!(batcher.start_height(StartHeightInput { height }).await, Err(expected_error));
}

#[rstest]
#[tokio::test]
async fn duplicate_start_height() {
    let mut batcher = create_batcher(MockDependencies::default()).await;

    let initial_height = StartHeightInput { height: INITIAL_HEIGHT };
    assert_eq!(batcher.start_height(initial_height.clone()).await, Ok(()));
    assert_eq!(batcher.start_height(initial_height).await, Err(BatcherError::HeightInProgress));
}

#[rstest]
#[tokio::test]
async fn no_active_height() {
    let mut batcher = create_batcher(MockDependencies::default()).await;

    // Calling `propose_block` and `validate_block` without starting a height should fail.

    let result = batcher.propose_block(propose_block_input(PROPOSAL_ID)).await;
    assert_eq!(result, Err(BatcherError::NoActiveHeight));

    let result = batcher.validate_block(validate_block_input(PROPOSAL_ID)).await;
    assert_eq!(result, Err(BatcherError::NoActiveHeight));
}

#[rstest]
#[case::proposer(true)]
#[case::validator(false)]
#[tokio::test]
async fn l1_handler_provider_not_ready(#[case] proposer: bool) {
    let mut deps = MockDependencies::default();
    deps.l1_provider_client.expect_start_block().returning(|_, _| {
        // The heights are not important for the test.
        let err =
            L1ProviderError::UnexpectedHeight { expected: INITIAL_HEIGHT, got: INITIAL_HEIGHT };
        Err(err.into())
    });
    let mut batcher = create_batcher(deps).await;
    assert_eq!(batcher.start_height(StartHeightInput { height: INITIAL_HEIGHT }).await, Ok(()));

    if proposer {
        assert_eq!(
            batcher.propose_block(propose_block_input(PROPOSAL_ID)).await,
            Err(BatcherError::NotReady)
        );
    } else {
        assert_eq!(
            batcher.validate_block(validate_block_input(PROPOSAL_ID)).await,
            Err(BatcherError::NotReady)
        );
    }
}

#[rstest]
#[tokio::test]
async fn consecutive_heights_success() {
    let mut storage_reader = MockBatcherStorageReaderTrait::new();
    storage_reader.expect_height().times(1).returning(|| Ok(INITIAL_HEIGHT)); // metrics registration
    storage_reader.expect_height().times(1).returning(|| Ok(INITIAL_HEIGHT)); // first start_height
    storage_reader.expect_height().times(1).returning(|| Ok(INITIAL_HEIGHT.unchecked_next())); // second start_height

    let mut block_builder_factory = MockBlockBuilderFactoryTrait::new();
    for _ in 0..2 {
        mock_create_builder_for_propose_block(
            &mut block_builder_factory,
            vec![],
            Ok(BlockExecutionArtifacts::create_for_testing()),
        );
    }

    let mut l1_provider_client = MockL1ProviderClient::new();
    l1_provider_client.expect_start_block().times(2).returning(|_, _| Ok(()));
    let mut batcher = create_batcher(MockDependencies {
        block_builder_factory,
        storage_reader,
        l1_provider_client,
        ..Default::default()
    })
    .await;

    // Prepare the propose_block requests for the first and the second heights.
    let first_propose_block_input = propose_block_input(PROPOSAL_ID);
    let mut second_propose_block_input = first_propose_block_input.clone();
    second_propose_block_input.block_info.block_number = INITIAL_HEIGHT.unchecked_next();

    // Start the first height and propose block.
    batcher.start_height(StartHeightInput { height: INITIAL_HEIGHT }).await.unwrap();
    batcher.propose_block(first_propose_block_input).await.unwrap();

    // Start the second height, and make sure the previous height proposal is cleared, by trying to
    // create a proposal with the same ID.
    batcher
        .start_height(StartHeightInput { height: INITIAL_HEIGHT.unchecked_next() })
        .await
        .unwrap();
    batcher.propose_block(second_propose_block_input).await.unwrap();
}

#[rstest]
#[tokio::test]
async fn validate_block_full_flow() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    let mut batcher =
        batcher_with_active_validate_block(Ok(BlockExecutionArtifacts::create_for_testing())).await;
    let metrics = recorder.handle().render();
    assert_proposal_metrics(&metrics, 1, 0, 0, 0);

    let send_proposal_input_txs = SendProposalContentInput {
        proposal_id: PROPOSAL_ID,
        content: SendProposalContent::Txs(test_txs(0..1)),
    };
    assert_eq!(
        batcher.send_proposal_content(send_proposal_input_txs).await.unwrap(),
        SendProposalContentResponse { response: ProposalStatus::Processing }
    );

    let finish_proposal =
        SendProposalContentInput { proposal_id: PROPOSAL_ID, content: SendProposalContent::Finish };
    assert_eq!(
        batcher.send_proposal_content(finish_proposal).await.unwrap(),
        SendProposalContentResponse { response: ProposalStatus::Finished(proposal_commitment()) }
    );
    let metrics = recorder.handle().render();
    assert_proposal_metrics(&metrics, 1, 1, 0, 0);
}

#[rstest]
#[case::send_txs(SendProposalContent::Txs(test_txs(0..1)))]
#[case::send_finish(SendProposalContent::Finish)]
#[case::send_abort(SendProposalContent::Abort)]
#[tokio::test]
async fn send_content_to_unknown_proposal(#[case] content: SendProposalContent) {
    let mut batcher = create_batcher(MockDependencies::default()).await;

    let send_proposal_content_input =
        SendProposalContentInput { proposal_id: PROPOSAL_ID, content };
    let result = batcher.send_proposal_content(send_proposal_content_input).await;
    assert_eq!(result, Err(BatcherError::ProposalNotFound { proposal_id: PROPOSAL_ID }));
}

#[rstest]
#[case::send_txs(SendProposalContent::Txs(test_txs(0..1)), ProposalStatus::InvalidProposal)]
#[case::send_finish(SendProposalContent::Finish, ProposalStatus::InvalidProposal)]
#[case::send_abort(SendProposalContent::Abort, ProposalStatus::Aborted)]
#[tokio::test]
async fn send_content_to_an_invalid_proposal(
    #[case] content: SendProposalContent,
    #[case] response: ProposalStatus,
) {
    let mut batcher = batcher_with_active_validate_block(Err(BUILD_BLOCK_FAIL_ON_ERROR)).await;
    batcher.await_active_proposal().await;

    let send_proposal_content_input =
        SendProposalContentInput { proposal_id: PROPOSAL_ID, content };
    let result = batcher.send_proposal_content(send_proposal_content_input).await.unwrap();
    assert_eq!(result, SendProposalContentResponse { response });
}

#[rstest]
#[case::send_txs_after_finish(SendProposalContent::Finish, SendProposalContent::Txs(test_txs(0..1)))]
#[case::send_finish_after_finish(SendProposalContent::Finish, SendProposalContent::Finish)]
#[case::send_abort_after_finish(SendProposalContent::Finish, SendProposalContent::Abort)]
#[case::send_txs_after_abort(SendProposalContent::Abort, SendProposalContent::Txs(test_txs(0..1)))]
#[case::send_finish_after_abort(SendProposalContent::Abort, SendProposalContent::Finish)]
#[case::send_abort_after_abort(SendProposalContent::Abort, SendProposalContent::Abort)]
#[tokio::test]
async fn send_proposal_content_after_finish_or_abort(
    #[case] end_proposal_content: SendProposalContent,
    #[case] content: SendProposalContent,
) {
    let mut batcher =
        batcher_with_active_validate_block(Ok(BlockExecutionArtifacts::create_for_testing())).await;

    // End the proposal.
    let end_proposal =
        SendProposalContentInput { proposal_id: PROPOSAL_ID, content: end_proposal_content };
    batcher.send_proposal_content(end_proposal).await.unwrap();

    // Send another request.
    let send_proposal_content_input =
        SendProposalContentInput { proposal_id: PROPOSAL_ID, content };
    let result = batcher.send_proposal_content(send_proposal_content_input).await;
    assert_eq!(result, Err(BatcherError::ProposalNotFound { proposal_id: PROPOSAL_ID }));
}

#[rstest]
#[tokio::test]
async fn send_proposal_content_abort() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    let mut batcher = batcher_with_active_validate_block(Err(BlockBuilderError::Aborted)).await;
    let metrics = recorder.handle().render();
    assert_proposal_metrics(&metrics, 1, 0, 0, 0);

    let send_abort_proposal =
        SendProposalContentInput { proposal_id: PROPOSAL_ID, content: SendProposalContent::Abort };
    assert_eq!(
        batcher.send_proposal_content(send_abort_proposal).await.unwrap(),
        SendProposalContentResponse { response: ProposalStatus::Aborted }
    );

    // The block builder is running in a separate task, and the proposal metrics are emitted from
    // that task, so we need to wait for them (we don't have a way to wait for the completion of the
    // abort).
    // TODO(AlonH): Find a way to wait for the metrics to be emitted.
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let metrics = recorder.handle().render();
    assert_proposal_metrics(&metrics, 1, 0, 0, 1);
}

#[rstest]
#[tokio::test]
async fn propose_block_full_flow() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    // Expecting 3 chunks of streamed txs.
    let expected_streamed_txs = test_txs(0..STREAMING_CHUNK_SIZE * 2 + 1);

    let mut block_builder_factory = MockBlockBuilderFactoryTrait::new();
    mock_create_builder_for_propose_block(
        &mut block_builder_factory,
        expected_streamed_txs.clone(),
        Ok(BlockExecutionArtifacts::create_for_testing()),
    );

    let mut l1_provider_client = MockL1ProviderClient::new();
    l1_provider_client.expect_start_block().times(1).returning(|_, _| Ok(()));

    let mut batcher = create_batcher(MockDependencies {
        block_builder_factory,
        l1_provider_client,
        ..Default::default()
    })
    .await;

    batcher.start_height(StartHeightInput { height: INITIAL_HEIGHT }).await.unwrap();
    batcher.propose_block(propose_block_input(PROPOSAL_ID)).await.unwrap();

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

    let metrics = recorder.handle().render();
    assert_proposal_metrics(&metrics, 1, 1, 0, 0);
}

#[rstest]
#[tokio::test]
async fn get_height() {
    let mut storage_reader = MockBatcherStorageReaderTrait::new();
    storage_reader.expect_height().returning(|| Ok(INITIAL_HEIGHT));

    let batcher = create_batcher(MockDependencies { storage_reader, ..Default::default() }).await;

    let result = batcher.get_height().await.unwrap();
    assert_eq!(result, GetHeightResponse { height: INITIAL_HEIGHT });
}

#[rstest]
#[tokio::test]
async fn propose_block_without_retrospective_block_hash() {
    let mut storage_reader = MockBatcherStorageReaderTrait::new();
    storage_reader
        .expect_height()
        .returning(|| Ok(BlockNumber(constants::STORED_BLOCK_HASH_BUFFER)));

    let mut batcher =
        create_batcher(MockDependencies { storage_reader, ..Default::default() }).await;

    batcher
        .start_height(StartHeightInput { height: BlockNumber(constants::STORED_BLOCK_HASH_BUFFER) })
        .await
        .unwrap();
    let result = batcher.propose_block(propose_block_input(PROPOSAL_ID)).await;

    assert_matches!(result, Err(BatcherError::MissingRetrospectiveBlockHash));
}

#[rstest]
#[tokio::test]
async fn get_content_from_unknown_proposal() {
    let mut batcher = create_batcher(MockDependencies::default()).await;

    let get_proposal_content_input = GetProposalContentInput { proposal_id: PROPOSAL_ID };
    let result = batcher.get_proposal_content(get_proposal_content_input).await;
    assert_eq!(result, Err(BatcherError::ProposalNotFound { proposal_id: PROPOSAL_ID }));
}

#[rstest]
#[tokio::test]
async fn consecutive_proposal_generation_success() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    let mut block_builder_factory = MockBlockBuilderFactoryTrait::new();
    for _ in 0..2 {
        mock_create_builder_for_propose_block(
            &mut block_builder_factory,
            vec![],
            Ok(BlockExecutionArtifacts::create_for_testing()),
        );
        mock_create_builder_for_validate_block(
            &mut block_builder_factory,
            Ok(BlockExecutionArtifacts::create_for_testing()),
        );
    }
    let mut l1_provider_client = MockL1ProviderClient::new();
    l1_provider_client.expect_start_block().times(4).returning(|_, _| Ok(()));
    let mut batcher = create_batcher(MockDependencies {
        block_builder_factory,
        l1_provider_client,
        ..Default::default()
    })
    .await;

    batcher.start_height(StartHeightInput { height: INITIAL_HEIGHT }).await.unwrap();

    // Make sure we can generate 4 consecutive proposals.
    for i in 0..2 {
        batcher.propose_block(propose_block_input(ProposalId(2 * i))).await.unwrap();
        batcher.await_active_proposal().await;

        batcher.validate_block(validate_block_input(ProposalId(2 * i + 1))).await.unwrap();
        let finish_proposal = SendProposalContentInput {
            proposal_id: ProposalId(2 * i + 1),
            content: SendProposalContent::Finish,
        };
        batcher.send_proposal_content(finish_proposal).await.unwrap();
        batcher.await_active_proposal().await;
    }

    let metrics = recorder.handle().render();
    assert_proposal_metrics(&metrics, 4, 4, 0, 0);
}

#[rstest]
#[tokio::test]
async fn concurrent_proposals_generation_fail() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    let mut batcher =
        batcher_with_active_validate_block(Ok(BlockExecutionArtifacts::create_for_testing())).await;

    // Make sure another proposal can't be generated while the first one is still active.
    let result = batcher.propose_block(propose_block_input(ProposalId(1))).await;

    assert_matches!(result, Err(BatcherError::AnotherProposalInProgress { .. }));

    // Finish the first proposal.
    batcher
        .send_proposal_content(SendProposalContentInput {
            proposal_id: ProposalId(0),
            content: SendProposalContent::Finish,
        })
        .await
        .unwrap();
    batcher.await_active_proposal().await;

    let metrics = recorder.handle().render();
    assert_proposal_metrics(&metrics, 2, 1, 1, 0);
}

#[rstest]
#[tokio::test]
async fn add_sync_block() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    let mut mock_dependencies = MockDependencies::default();

    mock_dependencies
        .storage_writer
        .expect_commit_proposal()
        .times(1)
        .with(eq(INITIAL_HEIGHT), eq(test_state_diff()))
        .returning(|_, _| Ok(()));

    mock_dependencies
        .mempool_client
        .expect_commit_block()
        .times(1)
        .with(eq(CommitBlockArgs {
            address_to_nonce: test_contract_nonces(),
            rejected_tx_hashes: [].into(),
        }))
        .returning(|_| Ok(()));

    mock_dependencies
        .l1_provider_client
        .expect_commit_block()
        .times(1)
        .with(eq(vec![]), eq(INITIAL_HEIGHT))
        .returning(|_, _| Ok(()));

    let mut batcher = create_batcher(mock_dependencies).await;

    let sync_block = SyncBlock {
        block_header_without_hash: BlockHeaderWithoutHash {
            block_number: INITIAL_HEIGHT,
            ..Default::default()
        },
        state_diff: test_state_diff(),
        transaction_hashes: test_tx_hashes().into_iter().collect(),
    };
    batcher.add_sync_block(sync_block).await.unwrap();
    let metrics = recorder.handle().render();
    assert_eq!(
        STORAGE_HEIGHT.parse_numeric_metric::<u64>(&metrics),
        Some(INITIAL_HEIGHT.unchecked_next().0)
    );
}

#[rstest]
#[tokio::test]
async fn add_sync_block_mismatch_block_number() {
    let mut batcher = create_batcher(MockDependencies::default()).await;

    let sync_block = SyncBlock {
        block_header_without_hash: BlockHeaderWithoutHash {
            block_number: INITIAL_HEIGHT.unchecked_next(),
            ..Default::default()
        },
        ..Default::default()
    };
    let result = batcher.add_sync_block(sync_block).await;
    assert_eq!(
        result,
        Err(BatcherError::StorageHeightMarkerMismatch {
            marker_height: BlockNumber(3),
            requested_height: BlockNumber(4)
        })
    )
}

#[tokio::test]
async fn revert_block() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    let mut mock_dependencies = MockDependencies::default();

    mock_dependencies
        .storage_writer
        .expect_revert_block()
        .times(1)
        .with(eq(LATEST_BLOCK_IN_STORAGE))
        .returning(|_| ());

    let mut batcher = create_batcher(mock_dependencies).await;

    let metrics = recorder.handle().render();
    assert_eq!(STORAGE_HEIGHT.parse_numeric_metric::<u64>(&metrics), Some(INITIAL_HEIGHT.0));

    let revert_input = RevertBlockInput { height: LATEST_BLOCK_IN_STORAGE };
    batcher.revert_block(revert_input).await.unwrap();

    let metrics = recorder.handle().render();
    assert_eq!(STORAGE_HEIGHT.parse_numeric_metric::<u64>(&metrics), Some(INITIAL_HEIGHT.0 - 1));
}

#[tokio::test]
async fn revert_block_mismatch_block_number() {
    let mut batcher = create_batcher(MockDependencies::default()).await;

    let revert_input = RevertBlockInput { height: INITIAL_HEIGHT };
    let result = batcher.revert_block(revert_input).await;
    assert_eq!(
        result,
        Err(BatcherError::StorageHeightMarkerMismatch {
            marker_height: BlockNumber(3),
            requested_height: BlockNumber(3)
        })
    )
}

#[tokio::test]
async fn revert_block_empty_storage() {
    let mut storage_reader = MockBatcherStorageReaderTrait::new();
    storage_reader.expect_height().returning(|| Ok(BlockNumber(0)));

    let mock_dependencies = MockDependencies { storage_reader, ..Default::default() };
    let mut batcher = create_batcher(mock_dependencies).await;

    let revert_input = RevertBlockInput { height: BlockNumber(0) };
    let result = batcher.revert_block(revert_input).await;
    assert_eq!(
        result,
        Err(BatcherError::StorageHeightMarkerMismatch {
            marker_height: BlockNumber(0),
            requested_height: BlockNumber(0)
        })
    );
}

#[rstest]
#[tokio::test]
async fn decision_reached() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    let mut mock_dependencies = MockDependencies::default();
    let expected_artifacts = BlockExecutionArtifacts::create_for_testing();

    mock_dependencies
        .mempool_client
        .expect_commit_block()
        .times(1)
        .with(eq(CommitBlockArgs {
            address_to_nonce: expected_artifacts.address_to_nonce(),
            rejected_tx_hashes: expected_artifacts.rejected_tx_hashes.clone(),
        }))
        .returning(|_| Ok(()));

    mock_dependencies
        .l1_provider_client
        .expect_start_block()
        .times(1)
        .with(eq(SessionState::Propose), eq(INITIAL_HEIGHT))
        .returning(|_, _| Ok(()));

    mock_dependencies
        .l1_provider_client
        .expect_commit_block()
        .times(1)
        .with(eq(vec![]), eq(INITIAL_HEIGHT))
        .returning(|_, _| Ok(()));

    mock_dependencies
        .storage_writer
        .expect_commit_proposal()
        .times(1)
        .with(eq(INITIAL_HEIGHT), eq(expected_artifacts.thin_state_diff()))
        .returning(|_, _| Ok(()));

    mock_create_builder_for_propose_block(
        &mut mock_dependencies.block_builder_factory,
        vec![],
        Ok(BlockExecutionArtifacts::create_for_testing()),
    );

    let decision_reached_response = batcher_propose_and_commit_block(mock_dependencies).await;

    verify_decision_reached_response(&decision_reached_response, &expected_artifacts);

    let metrics = recorder.handle().render();
    assert_eq!(
        STORAGE_HEIGHT.parse_numeric_metric::<u64>(&metrics),
        Some(INITIAL_HEIGHT.unchecked_next().0)
    );
    assert_eq!(
        BATCHED_TRANSACTIONS.parse_numeric_metric::<usize>(&metrics),
        Some(expected_artifacts.execution_infos.len())
    );
    assert_eq!(
        REJECTED_TRANSACTIONS.parse_numeric_metric::<usize>(&metrics),
        Some(expected_artifacts.rejected_tx_hashes.len())
    );
}

#[rstest]
#[tokio::test]
async fn decision_reached_no_executed_proposal() {
    let expected_error = BatcherError::ExecutedProposalNotFound { proposal_id: PROPOSAL_ID };

    let mut batcher = create_batcher(MockDependencies::default()).await;
    batcher.start_height(StartHeightInput { height: INITIAL_HEIGHT }).await.unwrap();

    let decision_reached_result =
        batcher.decision_reached(DecisionReachedInput { proposal_id: PROPOSAL_ID }).await;
    assert_eq!(decision_reached_result, Err(expected_error));
}

// Test that the batcher returns the execution_infos in the same order as returned from the
// block_builder. It is crucial that the execution_infos will be ordered in the same order as
// the transactions in the block for the correct execution of starknet.
// This test together with [block_builder_test::test_execution_info_order] covers this requirement.
#[tokio::test]
async fn test_execution_info_order_is_kept() {
    let mut mock_dependencies = MockDependencies::default();
    mock_dependencies.l1_provider_client.expect_start_block().returning(|_, _| Ok(()));
    mock_dependencies.mempool_client.expect_commit_block().returning(|_| Ok(()));
    mock_dependencies.l1_provider_client.expect_commit_block().returning(|_, _| Ok(()));
    mock_dependencies.storage_writer.expect_commit_proposal().returning(|_, _| Ok(()));

    let block_builder_result = BlockExecutionArtifacts::create_for_testing();
    // Check that the execution_infos were initiated properly for this test.
    verify_indexed_execution_infos(&block_builder_result.execution_infos);

    mock_create_builder_for_propose_block(
        &mut mock_dependencies.block_builder_factory,
        vec![],
        Ok(block_builder_result.clone()),
    );

    let decision_reached_response = batcher_propose_and_commit_block(mock_dependencies).await;

    // Verify that the execution_infos are in the same order as returned from the block_builder.
    let expected_execution_infos: Vec<TransactionExecutionInfo> =
        block_builder_result.execution_infos.into_values().collect();
    assert_eq!(decision_reached_response.central_objects.execution_infos, expected_execution_infos);
}

#[tokio::test]
async fn mempool_not_ready() {
    let mut mock_dependencies = MockDependencies::default();
    mock_dependencies.mempool_client.checkpoint();
    mock_dependencies.mempool_client.expect_update_gas_price().returning(|_| {
        Err(MempoolClientError::ClientError(ClientError::CommunicationFailure("".to_string())))
    });
    mock_dependencies.l1_provider_client.expect_start_block().returning(|_, _| Ok(()));

    let mut batcher = create_batcher(mock_dependencies).await;
    batcher.start_height(StartHeightInput { height: INITIAL_HEIGHT }).await.unwrap();
    let result = batcher.propose_block(propose_block_input(PROPOSAL_ID)).await;
    assert_eq!(result, Err(BatcherError::InternalError));
}
