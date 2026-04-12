use std::fmt::Debug;
use std::hash::Hash;
use std::sync::Arc;

use apollo_batcher_config::config::{
    BatcherConfig,
    BatcherDynamicConfig,
    BatcherStaticConfig,
    BlockBuilderConfig,
};
use apollo_batcher_types::batcher_types::{
    DecisionReachedInput,
    DecisionReachedResponse,
    FinishProposalInput,
    FinishProposalStatus,
    FinishedProposalInfo,
    GetHeightResponse,
    GetProposalContent,
    GetProposalContentInput,
    GetProposalContentResponse,
    ProposalCommitment,
    ProposalId,
    RevertBlockInput,
    SendTxsForProposalInput,
    SendTxsForProposalStatus,
    StartHeightInput,
    ValidateBlockInput,
};
use apollo_batcher_types::errors::BatcherError;
use apollo_committer_types::committer_types::CommitBlockRequest;
use apollo_config_manager_types::communication::MockConfigManagerReaderClient;
use apollo_infra::component_client::ClientError;
use apollo_infra::component_definitions::ComponentStarter;
use apollo_l1_events_types::errors::{L1EventsProviderClientError, L1EventsProviderError};
use apollo_l1_events_types::{MockL1EventsProviderClient, SessionState};
use apollo_mempool_types::communication::{
    MempoolClientError,
    MockMempoolClient,
    SharedMempoolClient,
};
use apollo_mempool_types::mempool_types::CommitBlockArgs;
use apollo_state_sync_types::state_sync_types::SyncBlock;
use apollo_storage::db::DbError;
use apollo_storage::test_utils::get_test_storage;
use apollo_storage::{StorageError, StorageReader, StorageWriter};
use assert_matches::assert_matches;
use blockifier::abi::constants;
use indexmap::{indexmap, IndexMap, IndexSet};
use metrics_exporter_prometheus::PrometheusBuilder;
use mockall::predicate::{always, eq};
use rstest::rstest;
use starknet_api::block::{
    BlockHash,
    BlockHeaderWithoutHash,
    BlockInfo,
    BlockNumber,
    StarknetVersion,
};
use starknet_api::block_hash::block_hash_calculator::{
    calculate_block_hash,
    PartialBlockHash,
    PartialBlockHashComponents,
};
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::core::{ClassHash, CompiledClassHash, GlobalRoot, Nonce};
use starknet_api::state::ThinStateDiff;
use starknet_api::transaction::TransactionHash;
use starknet_api::tx_hash;
use starknet_types_core::felt::Felt;
use tempfile::TempDir;
use validator::Validate;

use crate::batcher::{
    finished_proposal_info_from_artifacts,
    Batcher,
    BatcherStorageReader,
    BatcherStorageWriter,
    MockBatcherStorageReader,
    MockBatcherStorageWriter,
    StorageCommitmentBlockHash,
};
use crate::block_builder::{
    AbortSignalSender,
    BlockBuilderError,
    BlockBuilderResult,
    BlockExecutionArtifacts,
    MockBlockBuilderFactoryTrait,
};
use crate::commitment_manager::commitment_manager_impl::CommitmentManager;
use crate::commitment_manager::types::CommitterTaskInput;
use crate::metrics::{
    BATCHED_TRANSACTIONS,
    BUILDING_HEIGHT,
    LAST_SYNCED_BLOCK_HEIGHT,
    PROPOSAL_ABORTED,
    PROPOSAL_FAILED,
    PROPOSAL_STARTED,
    PROPOSAL_SUCCEEDED,
    REJECTED_TRANSACTIONS,
    REVERTED_BLOCKS,
    REVERTED_TRANSACTIONS,
    SYNCED_TRANSACTIONS,
};
use crate::test_utils::{
    get_number_of_items_in_channel_from_receiver,
    propose_block_input,
    test_contract_nonces,
    test_l1_handler_txs,
    test_state_diff,
    test_txs,
    verify_indexed_execution_infos,
    wait_for_n_items,
    FakeProposeBlockBuilder,
    FakeValidateBlockBuilder,
    MockClients,
    MockDependencies,
    BLOCK_GENERATION_TIMEOUT,
    BUILD_BLOCK_FAIL_ON_ERROR,
    DUMMY_BLOCK_HASH,
    DUMMY_FINAL_N_EXECUTED_TXS,
    FIRST_BLOCK_NUMBER_WITH_PARTIAL_BLOCK_HASH,
    INITIAL_HEIGHT,
    LATEST_BLOCK_IN_STORAGE,
    PROPOSAL_ID,
    STREAMING_CHUNK_SIZE,
};

fn get_test_state_diff(
    mut keys_stream: impl Iterator<Item = u64>,
    mut values_stream: impl Iterator<Item = u64>,
) -> ThinStateDiff {
    ThinStateDiff {
        deployed_contracts: indexmap! {
            (keys_stream.next().unwrap()).into() => ClassHash(values_stream.next().unwrap().into()),
            (keys_stream.next().unwrap()).into() => ClassHash(values_stream.next().unwrap().into()),
        },
        storage_diffs: indexmap! {
            (keys_stream.next().unwrap()).into() => indexmap! {
                (keys_stream.next().unwrap()).into() => (values_stream.next().unwrap()).into(),
                (keys_stream.next().unwrap()).into() => values_stream.next().unwrap().into(),
            },
        },
        class_hash_to_compiled_class_hash: indexmap! {
            ClassHash(keys_stream.next().unwrap().into()) =>
                CompiledClassHash(values_stream.next().unwrap().into()),
            ClassHash(keys_stream.next().unwrap().into()) =>
                CompiledClassHash(values_stream.next().unwrap().into()),
        },
        nonces: indexmap! {
            (keys_stream.next().unwrap()).into() => Nonce(values_stream.next().unwrap().into()),
            (keys_stream.next().unwrap()).into() => Nonce(values_stream.next().unwrap().into()),
        },
        deprecated_declared_classes: vec![
            ClassHash(keys_stream.next().unwrap().into()),
            ClassHash(keys_stream.next().unwrap().into()),
        ],
    }
}

/// The keys in each consecutive state diff are overlapping, for each map in the state diff.
/// If in block A the keys are x, x+1, then in block A+1 the keys are x+1, x+2.
fn get_overlapping_state_diffs(n_state_diffs: u64) -> Vec<ThinStateDiff> {
    let mut state_diffs = Vec::new();
    for i in 0..n_state_diffs {
        state_diffs.push(get_test_state_diff(i.., (i * 100)..));
    }
    state_diffs
}

fn write_state_diff(batcher: &mut Batcher, height: BlockNumber, state_diff: &ThinStateDiff) {
    batcher
        .storage_writer
        .commit_proposal(
            height,
            state_diff.clone(),
            StorageCommitmentBlockHash::Partial(PartialBlockHashComponents::default()),
        )
        .expect("set_state_diff failed");
}

async fn finished_proposal_info() -> FinishedProposalInfo {
    let artifacts = BlockExecutionArtifacts::create_for_testing().await;
    FinishedProposalInfo::new(
        finished_proposal_info_from_artifacts(&artifacts),
        Some(parent_proposal_commitment()),
    )
}

fn parent_proposal_commitment() -> ProposalCommitment {
    ProposalCommitment {
        partial_block_hash: PartialBlockHash::from_partial_block_hash_components(
            &PartialBlockHashComponents::default(),
        )
        .expect("default partial block hash components are valid"),
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

struct MockDependenciesWithRealStorage {
    storage_reader: StorageReader,
    storage_writer: StorageWriter,
    clients: MockClients,
    batcher_config: BatcherConfig,
    _temp_dir: TempDir, // Keep the temp dir alive.
}

impl Default for MockDependenciesWithRealStorage {
    fn default() -> Self {
        let ((storage_reader, storage_writer), temp_dir) = get_test_storage();

        Self {
            storage_reader,
            storage_writer,
            clients: MockClients::default(),
            batcher_config: BatcherConfig {
                static_config: BatcherStaticConfig {
                    outstream_content_buffer_size: STREAMING_CHUNK_SIZE,
                    ..Default::default()
                },
                ..Default::default()
            },
            _temp_dir: temp_dir,
        }
    }
}

async fn create_batcher(mock_dependencies: MockDependencies) -> Batcher {
    create_batcher_impl(
        Arc::new(mock_dependencies.storage_reader),
        Box::new(mock_dependencies.storage_writer),
        mock_dependencies.clients,
        mock_dependencies.batcher_config,
    )
    .await
}

async fn create_batcher_with_real_storage(
    mock_dependencies: MockDependenciesWithRealStorage,
) -> Batcher {
    create_batcher_impl(
        Arc::new(mock_dependencies.storage_reader),
        Box::new(mock_dependencies.storage_writer),
        mock_dependencies.clients,
        mock_dependencies.batcher_config,
    )
    .await
}

async fn create_batcher_impl<R: BatcherStorageReader + 'static>(
    storage_reader: Arc<R>,
    storage_writer: Box<dyn BatcherStorageWriter>,
    clients: MockClients,
    config: BatcherConfig,
) -> Batcher {
    let mempool_client: Option<SharedMempoolClient> = if config.static_config.validation_only {
        None
    } else {
        Some(Arc::new(clients.mempool_client))
    };
    let committer_client = Arc::new(clients.committer_client);
    let commitment_manager = CommitmentManager::create_commitment_manager(
        &config.static_config.commitment_manager_config,
        storage_reader.clone(),
        committer_client.clone(),
    )
    .await;

    let mut mock_config_manager = MockConfigManagerReaderClient::new();
    mock_config_manager
        .expect_get_batcher_dynamic_config()
        .returning(|| Ok(BatcherDynamicConfig::default()));

    let mut batcher = Batcher::new(
        config,
        storage_reader,
        storage_writer,
        committer_client,
        Arc::new(clients.l1_provider_client),
        mempool_client,
        Arc::new(mock_config_manager),
        Box::new(clients.block_builder_factory),
        Box::new(clients.pre_confirmed_block_writer_factory),
        commitment_manager,
        tokio::spawn(async {}).abort_handle(),
    );
    // Call post-creation functionality (e.g., metrics registration).
    batcher.start().await;
    batcher
}

fn abort_signal_sender() -> AbortSignalSender {
    tokio::sync::oneshot::channel().0
}

/// Calls `Batcher::new` with an explicit `mempool_client`, bypassing the auto-derivation in
/// `create_batcher_impl`. Used to test the consistency assert in `Batcher::new`.
async fn new_batcher_with_mempool_override(
    deps: MockDependencies,
    mempool_client: Option<SharedMempoolClient>,
) {
    let storage_reader = Arc::new(deps.storage_reader);
    let committer_client = Arc::new(deps.clients.committer_client);
    let commitment_manager = CommitmentManager::create_commitment_manager(
        &deps.batcher_config.static_config.commitment_manager_config,
        storage_reader.clone(),
        committer_client.clone(),
    )
    .await;
    let mut mock_config_manager = MockConfigManagerReaderClient::new();
    mock_config_manager
        .expect_get_batcher_dynamic_config()
        .returning(|| Ok(BatcherDynamicConfig::default()));
    Batcher::new(
        deps.batcher_config,
        storage_reader,
        Box::new(deps.storage_writer),
        committer_client,
        Arc::new(deps.clients.l1_provider_client),
        mempool_client,
        Arc::new(mock_config_manager),
        Box::new(deps.clients.block_builder_factory),
        Box::new(deps.clients.pre_confirmed_block_writer_factory),
        commitment_manager,
        tokio::spawn(async {}).abort_handle(),
    );
}

async fn batcher_propose_and_commit_block(
    mock_dependencies: MockDependencies,
) -> Result<DecisionReachedResponse, BatcherError> {
    let mut batcher = create_batcher(mock_dependencies).await;
    batcher.start_height(StartHeightInput { height: INITIAL_HEIGHT }).await.unwrap();
    batcher.propose_block(propose_block_input(PROPOSAL_ID)).await.unwrap();
    batcher.await_active_proposal(DUMMY_FINAL_N_EXECUTED_TXS).await.unwrap();
    batcher.decision_reached(DecisionReachedInput { proposal_id: PROPOSAL_ID }).await
}

fn mock_create_builder_for_validate_block(
    block_builder_factory: &mut MockBlockBuilderFactoryTrait,
    build_block_result: BlockBuilderResult<BlockExecutionArtifacts>,
) {
    block_builder_factory.expect_create_block_builder().times(1).return_once(
        |_, _, _, tx_provider, _, _, _, _| {
            let block_builder = FakeValidateBlockBuilder {
                tx_provider,
                build_block_result: Some(build_block_result),
            };
            Ok((Box::new(block_builder), abort_signal_sender()))
        },
    );
}

fn mock_storage_reader_for_revert() -> MockBatcherStorageReader {
    let mut storage_reader = MockBatcherStorageReader::new();
    storage_reader.expect_reversed_state_diff().returning(|_| Ok(test_state_diff()));
    storage_reader.expect_global_root_height().returning(|| Ok(INITIAL_HEIGHT));
    storage_reader.expect_global_root().returning(|_| Ok(Some(GlobalRoot::default())));
    storage_reader.expect_state_diff_height().returning(|| Ok(INITIAL_HEIGHT));
    storage_reader
}

fn mock_create_builder_for_propose_block(
    block_builder_factory: &mut MockBlockBuilderFactoryTrait,
    output_txs: Vec<InternalConsensusTransaction>,
    build_block_result: BlockBuilderResult<BlockExecutionArtifacts>,
) {
    block_builder_factory.expect_create_block_builder().times(1).return_once(
        move |_, _, _, tx_provider, output_content_sender, _, _, _| {
            let block_builder = FakeProposeBlockBuilder {
                output_content_sender: output_content_sender.unwrap(),
                output_txs,
                build_block_result: Some(build_block_result),
                tx_provider,
            };
            Ok((Box::new(block_builder), abort_signal_sender()))
        },
    );
}

async fn create_batcher_with_active_validate_block(
    build_block_result: BlockBuilderResult<BlockExecutionArtifacts>,
) -> Batcher {
    let mut block_builder_factory = MockBlockBuilderFactoryTrait::new();
    mock_create_builder_for_validate_block(&mut block_builder_factory, build_block_result);
    start_batcher_with_active_validate(block_builder_factory).await
}

async fn start_batcher_with_active_validate(
    block_builder_factory: MockBlockBuilderFactoryTrait,
) -> Batcher {
    let mut l1_provider_client = MockL1EventsProviderClient::new();
    l1_provider_client.expect_start_block().returning(|_, _| Ok(()));

    let mut batcher = create_batcher(MockDependencies {
        clients: MockClients { block_builder_factory, l1_provider_client, ..Default::default() },
        ..Default::default()
    })
    .await;

    batcher.start_height(StartHeightInput { height: INITIAL_HEIGHT }).await.unwrap();

    batcher.validate_block(validate_block_input(PROPOSAL_ID)).await.unwrap();

    batcher
}

fn test_tx_hashes() -> IndexSet<TransactionHash> {
    (0..5u8).map(|i| tx_hash!(i + 12)).collect()
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
        response.state_diff.class_hash_to_compiled_class_hash,
        expected_artifacts.commitment_state_diff.class_hash_to_compiled_class_hash
    );
    assert_eq!(
        response.state_diff.deployed_contracts,
        expected_artifacts.commitment_state_diff.address_to_class_hash
    );
    assert_eq!(response.central_objects.bouncer_weights, expected_artifacts.bouncer_weights);
    assert_eq!(
        response.central_objects.execution_infos.len(),
        expected_artifacts.execution_data.execution_infos_and_signatures.len()
    );
    for (tx_hash, info) in &response.central_objects.execution_infos {
        assert_eq!(
            info,
            &expected_artifacts.execution_data.execution_infos_and_signatures[tx_hash].0
        );
    }
    assert_eq!(
        response.central_objects.parent_proposal_commitment,
        Some(parent_proposal_commitment())
    );
}

fn assert_proposal_metrics(
    metrics: &str,
    expected_started_count: u64,
    expected_succeeded_count: u64,
    expected_failed_count: u64,
    expected_aborted_count: u64,
) {
    let n_expected_active_proposals = expected_started_count
        - (expected_succeeded_count + expected_failed_count + expected_aborted_count);
    assert!(n_expected_active_proposals <= 1);
    let actual_started_count = PROPOSAL_STARTED.parse_numeric_metric::<u64>(metrics);
    let actual_succeeded_count = PROPOSAL_SUCCEEDED.parse_numeric_metric::<u64>(metrics);
    let actual_failed_count = PROPOSAL_FAILED.parse_numeric_metric::<u64>(metrics);
    let actual_aborted_count = PROPOSAL_ABORTED.parse_numeric_metric::<u64>(metrics);

    assert_eq!(
        actual_started_count,
        Some(expected_started_count),
        "unexpected value proposal_started, expected {expected_started_count} got \
         {actual_started_count:?}",
    );
    assert_eq!(
        actual_succeeded_count,
        Some(expected_succeeded_count),
        "unexpected value proposal_succeeded, expected {expected_succeeded_count} got \
         {actual_succeeded_count:?}",
    );
    assert_eq!(
        actual_failed_count,
        Some(expected_failed_count),
        "unexpected value proposal_failed, expected {expected_failed_count} got \
         {actual_failed_count:?}",
    );
    assert_eq!(
        actual_aborted_count,
        Some(expected_aborted_count),
        "unexpected value proposal_aborted, expected {expected_aborted_count} got \
         {actual_aborted_count:?}",
    );
}

#[tokio::test]
async fn metrics_registered() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    let _batcher = create_batcher(MockDependencies::default()).await;
    let metrics = recorder.handle().render();
    assert_eq!(BUILDING_HEIGHT.parse_numeric_metric::<u64>(&metrics), Some(INITIAL_HEIGHT.0));
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
async fn ignore_l1_handler_provider_not_ready(#[case] proposer: bool) {
    let mut deps = MockDependencies::default();
    if proposer {
        mock_create_builder_for_propose_block(
            &mut deps.clients.block_builder_factory,
            vec![],
            Ok(BlockExecutionArtifacts::create_for_testing().await),
        );
    } else {
        mock_create_builder_for_validate_block(
            &mut deps.clients.block_builder_factory,
            Ok(BlockExecutionArtifacts::create_for_testing().await),
        );
    }
    deps.clients.l1_provider_client.expect_start_block().returning(|_, _| {
        // The heights are not important for the test.
        let err = L1EventsProviderError::UnexpectedHeight {
            expected_height: INITIAL_HEIGHT,
            got: INITIAL_HEIGHT,
        };
        Err(err.into())
    });
    let mut batcher = create_batcher(deps).await;
    assert_eq!(batcher.start_height(StartHeightInput { height: INITIAL_HEIGHT }).await, Ok(()));

    if proposer {
        batcher.propose_block(propose_block_input(PROPOSAL_ID)).await.unwrap();
    } else {
        batcher.validate_block(validate_block_input(PROPOSAL_ID)).await.unwrap();
    }
}

#[rstest]
#[tokio::test]
async fn consecutive_heights_success() {
    let mut storage_reader = MockBatcherStorageReader::new();
    storage_reader.expect_state_diff_height().times(1).returning(|| Ok(INITIAL_HEIGHT)); // batcher start
    storage_reader.expect_state_diff_height().times(1).returning(|| Ok(INITIAL_HEIGHT)); // first start_height
    storage_reader
        .expect_state_diff_height()
        .times(1)
        .returning(|| Ok(INITIAL_HEIGHT.unchecked_next())); // second start_height
    storage_reader.expect_global_root_height().returning(|| Ok(INITIAL_HEIGHT));

    let mut block_builder_factory = MockBlockBuilderFactoryTrait::new();
    for _ in 0..2 {
        mock_create_builder_for_propose_block(
            &mut block_builder_factory,
            vec![],
            Ok(BlockExecutionArtifacts::create_for_testing().await),
        );
    }

    let mut l1_provider_client = MockL1EventsProviderClient::new();
    l1_provider_client.expect_start_block().times(2).returning(|_, _| Ok(()));

    let mock_dependencies = MockDependencies {
        storage_reader,
        clients: MockClients { block_builder_factory, l1_provider_client, ..Default::default() },
        ..Default::default()
    };

    let mut batcher = create_batcher(mock_dependencies).await;

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
    let mut batcher = create_batcher_with_active_validate_block(Ok(
        BlockExecutionArtifacts::create_for_testing().await,
    ))
    .await;
    let metrics = recorder.handle().render();
    assert_proposal_metrics(&metrics, 1, 0, 0, 0);

    let send_txs_for_proposal_input =
        SendTxsForProposalInput { proposal_id: PROPOSAL_ID, txs: test_txs(0..1) };
    assert_eq!(
        batcher.send_txs_for_proposal(send_txs_for_proposal_input).await.unwrap(),
        SendTxsForProposalStatus::Processing
    );

    let finish_proposal = FinishProposalInput {
        proposal_id: PROPOSAL_ID,
        final_n_executed_txs: DUMMY_FINAL_N_EXECUTED_TXS,
    };
    let expected_info = finished_proposal_info().await;
    assert_eq!(
        batcher.finish_proposal(finish_proposal).await.unwrap(),
        FinishProposalStatus::Finished(expected_info)
    );
    let metrics = recorder.handle().render();
    assert_proposal_metrics(&metrics, 1, 1, 0, 0);
}

#[rstest]
#[case::abort(ProposalAction::Abort)]
#[case::finish(ProposalAction::FinishProposal)]
#[case::send_txs(ProposalAction::SendTxsForProposal)]
#[tokio::test]
async fn action_on_unknown_proposal(#[case] action: ProposalAction) {
    let mut batcher = create_batcher(MockDependencies::default()).await;

    let result = match action {
        ProposalAction::Abort => batcher.abort_proposal(PROPOSAL_ID).await,
        ProposalAction::FinishProposal => batcher
            .finish_proposal(FinishProposalInput {
                proposal_id: PROPOSAL_ID,
                final_n_executed_txs: DUMMY_FINAL_N_EXECUTED_TXS,
            })
            .await
            .map(|_| ()),
        ProposalAction::SendTxsForProposal => batcher
            .send_txs_for_proposal(SendTxsForProposalInput {
                proposal_id: PROPOSAL_ID,
                txs: test_txs(0..1),
            })
            .await
            .map(|_| ()),
    };
    assert_eq!(result, Err(BatcherError::ProposalNotFound { proposal_id: PROPOSAL_ID }));
}

#[rstest]
#[case::abort(ProposalAction::Abort)]
#[case::finish(ProposalAction::FinishProposal)]
#[case::send_txs(ProposalAction::SendTxsForProposal)]
#[tokio::test]
async fn action_on_invalid_proposal(#[case] action: ProposalAction) {
    let mut batcher =
        create_batcher_with_active_validate_block(Err(BUILD_BLOCK_FAIL_ON_ERROR)).await;
    batcher.await_active_proposal(DUMMY_FINAL_N_EXECUTED_TXS).await.unwrap();

    match action {
        ProposalAction::Abort => {
            assert_eq!(batcher.abort_proposal(PROPOSAL_ID).await, Ok(()));
        }
        ProposalAction::FinishProposal => {
            assert_eq!(
                batcher
                    .finish_proposal(FinishProposalInput {
                        proposal_id: PROPOSAL_ID,
                        final_n_executed_txs: DUMMY_FINAL_N_EXECUTED_TXS,
                    })
                    .await
                    .unwrap(),
                FinishProposalStatus::InvalidProposal("Block is full".to_string())
            );
        }
        ProposalAction::SendTxsForProposal => {
            assert_eq!(
                batcher
                    .send_txs_for_proposal(SendTxsForProposalInput {
                        proposal_id: PROPOSAL_ID,
                        txs: test_txs(0..1),
                    })
                    .await
                    .unwrap(),
                SendTxsForProposalStatus::InvalidProposal("Block is full".to_string())
            );
        }
    }
}

#[derive(Clone)]
enum EndProposalAction {
    Finish,
    Abort,
}

#[derive(Clone)]
enum ProposalAction {
    Abort,
    FinishProposal,
    SendTxsForProposal,
}

#[rstest]
#[case::abort_after_finish(EndProposalAction::Finish, ProposalAction::Abort)]
#[case::abort_after_abort(EndProposalAction::Abort, ProposalAction::Abort)]
#[case::finish_after_finish(EndProposalAction::Finish, ProposalAction::FinishProposal)]
#[case::finish_after_abort(EndProposalAction::Abort, ProposalAction::FinishProposal)]
#[case::send_txs_for_proposal_after_finish(
    EndProposalAction::Finish,
    ProposalAction::SendTxsForProposal
)]
#[case::send_txs_for_proposal_after_abort(
    EndProposalAction::Abort,
    ProposalAction::SendTxsForProposal
)]
#[tokio::test]
async fn proposal_not_found_after_terminal_action(
    #[case] end_action: EndProposalAction,
    #[case] after_end_action: ProposalAction,
) {
    let mut batcher = create_batcher_with_active_validate_block(Ok(
        BlockExecutionArtifacts::create_for_testing().await,
    ))
    .await;

    match end_action {
        EndProposalAction::Finish => {
            batcher
                .finish_proposal(FinishProposalInput {
                    proposal_id: PROPOSAL_ID,
                    final_n_executed_txs: DUMMY_FINAL_N_EXECUTED_TXS,
                })
                .await
                .unwrap();
        }
        EndProposalAction::Abort => {
            batcher.abort_proposal(PROPOSAL_ID).await.unwrap();
        }
    }

    let result = match after_end_action {
        ProposalAction::Abort => batcher.abort_proposal(PROPOSAL_ID).await,
        ProposalAction::FinishProposal => batcher
            .finish_proposal(FinishProposalInput {
                proposal_id: PROPOSAL_ID,
                final_n_executed_txs: DUMMY_FINAL_N_EXECUTED_TXS,
            })
            .await
            .map(|_| ()),
        ProposalAction::SendTxsForProposal => batcher
            .send_txs_for_proposal(SendTxsForProposalInput {
                proposal_id: PROPOSAL_ID,
                txs: test_txs(0..1),
            })
            .await
            .map(|_| ()),
    };
    assert_eq!(result, Err(BatcherError::ProposalNotFound { proposal_id: PROPOSAL_ID }));
}

#[rstest]
#[tokio::test]
async fn abort_proposal_test() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    let mut batcher =
        create_batcher_with_active_validate_block(Err(BlockBuilderError::Aborted)).await;
    let metrics = recorder.handle().render();
    assert_proposal_metrics(&metrics, 1, 0, 0, 0);

    batcher.abort_proposal(PROPOSAL_ID).await.unwrap();

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
        Ok(BlockExecutionArtifacts::create_for_testing().await),
    );

    let mut l1_provider_client = MockL1EventsProviderClient::new();
    l1_provider_client.expect_start_block().times(1).returning(|_, _| Ok(()));

    let mut batcher = create_batcher(MockDependencies {
        clients: MockClients { block_builder_factory, l1_provider_client, ..Default::default() },
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
    let expected_info = finished_proposal_info().await;
    assert_eq!(
        commitment,
        GetProposalContentResponse { content: GetProposalContent::Finished(expected_info) }
    );

    let exhausted =
        batcher.get_proposal_content(GetProposalContentInput { proposal_id: PROPOSAL_ID }).await;
    assert_matches!(exhausted, Err(BatcherError::ProposalNotFound { .. }));

    let metrics = recorder.handle().render();
    assert_proposal_metrics(&metrics, 1, 1, 0, 0);
}

#[rstest]
#[tokio::test]
async fn multiple_proposals_with_l1_every_n_proposals() {
    const N_PROPOSALS: usize = 4;
    const PROPOSALS_L1_MODULATOR: usize = 3;

    // Send a regular tx and an l1 handler tx.
    let mut expected_streamed_txs = test_txs(0..1);
    expected_streamed_txs.extend(test_l1_handler_txs(1..2));
    let mut block_builder_factory = MockBlockBuilderFactoryTrait::new();
    for _ in 0..N_PROPOSALS {
        mock_create_builder_for_propose_block(
            &mut block_builder_factory,
            expected_streamed_txs.clone(),
            Ok(BlockExecutionArtifacts::create_for_testing().await),
        );
    }

    let mut l1_provider_client = MockL1EventsProviderClient::new();
    l1_provider_client.expect_start_block().times(N_PROPOSALS).returning(|_, _| Ok(()));

    let mock_dependencies = MockDependencies {
        clients: MockClients { block_builder_factory, l1_provider_client, ..Default::default() },
        ..Default::default()
    };

    let mut batcher = create_batcher(mock_dependencies).await;
    // Only propose L1 txs every PROPOSALS_L1_MODULATOR proposals.
    batcher.config.static_config.propose_l1_txs_every = PROPOSALS_L1_MODULATOR.try_into().unwrap();

    for i in 0..N_PROPOSALS {
        batcher.start_height(StartHeightInput { height: INITIAL_HEIGHT }).await.unwrap();
        batcher.propose_block(propose_block_input(PROPOSAL_ID)).await.unwrap();
        let content = batcher
            .get_proposal_content(GetProposalContentInput { proposal_id: PROPOSAL_ID })
            .await
            .unwrap()
            .content;
        let txs = assert_matches!(content, GetProposalContent::Txs(txs) => txs);

        if (i + 1).is_multiple_of(PROPOSALS_L1_MODULATOR) {
            assert_eq!(txs, expected_streamed_txs);
        } else {
            assert_eq!(txs, test_txs(0..1));
        }

        batcher.await_active_proposal(DUMMY_FINAL_N_EXECUTED_TXS).await.unwrap();
        batcher.abort_active_height().await;
    }
}

#[rstest]
#[tokio::test]
async fn get_height() {
    let mut storage_reader = MockBatcherStorageReader::new();
    storage_reader.expect_state_diff_height().returning(|| Ok(INITIAL_HEIGHT));
    storage_reader.expect_global_root_height().returning(|| Ok(INITIAL_HEIGHT));

    let batcher = create_batcher(MockDependencies { storage_reader, ..Default::default() }).await;

    let result = batcher.get_height().await.unwrap();
    assert_eq!(result, GetHeightResponse { height: INITIAL_HEIGHT });
}

#[rstest]
#[tokio::test]
async fn propose_block_without_retrospective_block_hash() {
    let mut storage_reader = MockBatcherStorageReader::new();
    let initial_block_height = BlockNumber(constants::STORED_BLOCK_HASH_BUFFER);
    storage_reader.expect_state_diff_height().returning(move || Ok(initial_block_height));
    storage_reader.expect_global_root_height().returning(move || Ok(initial_block_height));

    let mut batcher =
        create_batcher(MockDependencies { storage_reader, ..Default::default() }).await;

    batcher.start_height(StartHeightInput { height: initial_block_height }).await.unwrap();
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
            Ok(BlockExecutionArtifacts::create_for_testing().await),
        );
        mock_create_builder_for_validate_block(
            &mut block_builder_factory,
            Ok(BlockExecutionArtifacts::create_for_testing().await),
        );
    }
    let mut l1_provider_client = MockL1EventsProviderClient::new();
    l1_provider_client.expect_start_block().times(4).returning(|_, _| Ok(()));

    let mut batcher = create_batcher(MockDependencies {
        clients: MockClients { block_builder_factory, l1_provider_client, ..Default::default() },
        ..Default::default()
    })
    .await;

    batcher.start_height(StartHeightInput { height: INITIAL_HEIGHT }).await.unwrap();

    // Make sure we can generate 4 consecutive proposals.
    for i in 0..2 {
        batcher.propose_block(propose_block_input(ProposalId(2 * i))).await.unwrap();
        batcher.await_active_proposal(DUMMY_FINAL_N_EXECUTED_TXS).await.unwrap();

        batcher.validate_block(validate_block_input(ProposalId(2 * i + 1))).await.unwrap();
        let finish_proposal = FinishProposalInput {
            proposal_id: ProposalId(2 * i + 1),
            final_n_executed_txs: DUMMY_FINAL_N_EXECUTED_TXS,
        };
        batcher.finish_proposal(finish_proposal).await.unwrap();
        batcher.await_active_proposal(DUMMY_FINAL_N_EXECUTED_TXS).await.unwrap();
    }

    let metrics = recorder.handle().render();
    assert_proposal_metrics(&metrics, 4, 4, 0, 0);
}

#[rstest]
#[tokio::test]
async fn concurrent_proposals_generation_fail() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    let mut block_builder_factory = MockBlockBuilderFactoryTrait::new();
    // Expecting the block builder factory to be called twice.
    for _ in 0..2 {
        mock_create_builder_for_validate_block(
            &mut block_builder_factory,
            Ok(BlockExecutionArtifacts::create_for_testing().await),
        );
    }
    let mut batcher = start_batcher_with_active_validate(block_builder_factory).await;

    // Make sure another proposal can't be generated while the first one is still active.
    let result = batcher.propose_block(propose_block_input(ProposalId(1))).await;

    assert_matches!(result, Err(BatcherError::AnotherProposalInProgress { .. }));

    // Finish the first proposal.
    batcher
        .finish_proposal(FinishProposalInput {
            proposal_id: ProposalId(0),
            final_n_executed_txs: DUMMY_FINAL_N_EXECUTED_TXS,
        })
        .await
        .unwrap();
    batcher.await_active_proposal(DUMMY_FINAL_N_EXECUTED_TXS).await.unwrap();

    let metrics = recorder.handle().render();
    assert_proposal_metrics(&metrics, 2, 1, 1, 0);
}

#[rstest]
#[tokio::test]
async fn proposal_startup_failure_allows_new_proposals() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    let mut block_builder_factory = MockBlockBuilderFactoryTrait::new();
    mock_create_builder_for_validate_block(
        &mut block_builder_factory,
        Ok(BlockExecutionArtifacts::create_for_testing().await),
    );
    let mut l1_provider_client = MockL1EventsProviderClient::new();
    l1_provider_client.expect_start_block().returning(|_, _| Ok(()));
    let mut mempool_client = MockMempoolClient::new();
    let expected_gas_price =
        propose_block_input(PROPOSAL_ID).block_info.gas_prices.strk_gas_prices.l2_gas_price.get();
    let error = MempoolClientError::ClientError(ClientError::CommunicationFailure(
        "Mempool not ready".to_string(),
    ));
    mempool_client
        .expect_update_gas_price()
        .with(eq(expected_gas_price))
        .return_once(|_| Err(error));
    mempool_client.expect_update_gas_price().with(eq(expected_gas_price)).return_once(|_| Ok(()));
    mempool_client.expect_commit_block().with(eq(CommitBlockArgs::default())).returning(|_| Ok(()));

    let mut batcher = create_batcher(MockDependencies {
        clients: MockClients {
            block_builder_factory,
            l1_provider_client,
            mempool_client,
            ..Default::default()
        },
        ..Default::default()
    })
    .await;

    batcher.start_height(StartHeightInput { height: INITIAL_HEIGHT }).await.unwrap();

    batcher
        .propose_block(propose_block_input(ProposalId(0)))
        .await
        .expect_err("Expected to fail because of the first MempoolClient error");

    batcher.validate_block(validate_block_input(ProposalId(1))).await.expect("Expected to succeed");
    batcher
        .finish_proposal(FinishProposalInput {
            proposal_id: ProposalId(1),
            final_n_executed_txs: DUMMY_FINAL_N_EXECUTED_TXS,
        })
        .await
        .unwrap();
    batcher.await_active_proposal(DUMMY_FINAL_N_EXECUTED_TXS).await.unwrap();

    let metrics = recorder.handle().render();
    assert_proposal_metrics(&metrics, 2, 1, 1, 0);
}

#[rstest]
#[case::new_sync_block(INITIAL_HEIGHT, Some(PartialBlockHashComponents {
    block_number: INITIAL_HEIGHT,
    ..Default::default()
}))]
#[case::old_sync_block(FIRST_BLOCK_NUMBER_WITH_PARTIAL_BLOCK_HASH.prev().unwrap(), None)]
#[tokio::test]
async fn add_sync_block(
    #[case] block_number: BlockNumber,
    #[case] partial_block_hash_components: Option<PartialBlockHashComponents>,
) {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    let l1_transaction_hashes = test_tx_hashes();
    let (starknet_version, block_header_commitments, storage_commitment_block_hash) =
        if let Some(ref partial_block_hash_components) = partial_block_hash_components {
            (
                StarknetVersion::LATEST,
                Some(Default::default()),
                StorageCommitmentBlockHash::Partial(partial_block_hash_components.clone()),
            )
        } else {
            (
                StarknetVersion::V0_13_1,
                None,
                StorageCommitmentBlockHash::ParentHash(BlockHash::default()),
            )
        };

    let mut mock_clients = MockClients::default();

    let mut storage_reader = MockBatcherStorageReader::new();
    storage_reader.expect_state_diff_height().returning(move || Ok(block_number));
    storage_reader.expect_global_root_height().returning(move || Ok(block_number));

    let mut storage_writer = MockBatcherStorageWriter::new();
    storage_writer
        .expect_commit_proposal()
        .times(1)
        .with(eq(block_number), eq(test_state_diff()), eq(storage_commitment_block_hash))
        .returning(|_, _, _| Ok(()));

    mock_clients
        .mempool_client
        .expect_commit_block()
        .times(1)
        .with(eq(CommitBlockArgs {
            address_to_nonce: test_contract_nonces(),
            rejected_tx_hashes: [].into(),
        }))
        .returning(|_| Ok(()));

    mock_clients
        .l1_provider_client
        .expect_commit_block()
        .times(1)
        .with(eq(l1_transaction_hashes.clone()), eq(IndexSet::new()), eq(block_number))
        .returning(|_, _, _| Ok(()));

    let mock_dependencies = MockDependencies {
        storage_reader,
        storage_writer,
        clients: mock_clients,
        ..Default::default()
    };

    let mut batcher = create_batcher(mock_dependencies).await;

    let n_synced_transactions = l1_transaction_hashes.len();

    let sync_block = SyncBlock {
        block_header_without_hash: BlockHeaderWithoutHash {
            block_number,
            starknet_version,
            ..Default::default()
        },
        state_diff: test_state_diff(),
        l1_transaction_hashes: l1_transaction_hashes.into_iter().collect(),
        block_header_commitments,
        ..Default::default()
    };
    batcher.add_sync_block(sync_block).await.unwrap();
    let metrics = recorder.handle().render();
    assert_eq!(
        BUILDING_HEIGHT.parse_numeric_metric::<u64>(&metrics),
        Some(block_number.unchecked_next().0)
    );
    let metrics = recorder.handle().render();
    assert_eq!(
        LAST_SYNCED_BLOCK_HEIGHT.parse_numeric_metric::<u64>(&metrics),
        Some(block_number.0)
    );
    assert_eq!(
        SYNCED_TRANSACTIONS.parse_numeric_metric::<usize>(&metrics),
        Some(n_synced_transactions)
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
        block_header_commitments: Some(Default::default()),
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

#[rstest]
#[tokio::test]
async fn add_sync_block_missing_block_header_commitments() {
    let mut storage_reader = MockBatcherStorageReader::new();
    storage_reader.expect_state_diff_height().returning(|| Ok(INITIAL_HEIGHT));
    storage_reader.expect_global_root_height().returning(|| Ok(INITIAL_HEIGHT));
    let mock_dependencies = MockDependencies { storage_reader, ..Default::default() };
    let mut batcher = create_batcher(mock_dependencies).await;

    let sync_block = SyncBlock {
        block_header_without_hash: BlockHeaderWithoutHash {
            block_number: INITIAL_HEIGHT,
            starknet_version: StarknetVersion::LATEST,
            ..Default::default()
        },
        state_diff: Default::default(),
        account_transaction_hashes: Default::default(),
        l1_transaction_hashes: Default::default(),
        block_header_commitments: None,
    };
    let result = batcher.add_sync_block(sync_block).await;
    assert_eq!(result, Err(BatcherError::MissingHeaderCommitments { block_number: INITIAL_HEIGHT }))
}

#[rstest]
#[tokio::test]
#[should_panic(expected = "is at least the first block configured to include a partial hash")]
async fn add_sync_block_missing_block_header_commitments_for_new_block() {
    let block_number = FIRST_BLOCK_NUMBER_WITH_PARTIAL_BLOCK_HASH.unchecked_next();
    let mut storage_reader = MockBatcherStorageReader::new();
    storage_reader.expect_state_diff_height().returning(move || Ok(block_number));
    storage_reader.expect_global_root_height().returning(move || Ok(block_number));
    let mock_dependencies = MockDependencies { storage_reader, ..Default::default() };

    let mut batcher = create_batcher(mock_dependencies).await;

    // Block number > FIRST_BLOCK_NUMBER_WITH_PARTIAL_BLOCK_HASH but starknet_version does not
    // have partial block hash components, and block_header_commitments is None.
    // This should trigger the assertion.
    let sync_block = SyncBlock {
        block_header_without_hash: BlockHeaderWithoutHash {
            block_number,
            starknet_version: StarknetVersion::V0_13_1,
            ..Default::default()
        },
        state_diff: Default::default(),
        account_transaction_hashes: Default::default(),
        l1_transaction_hashes: Default::default(),
        block_header_commitments: None,
    };
    let _ = batcher.add_sync_block(sync_block).await;
}

#[rstest]
#[tokio::test]
async fn add_sync_block_for_first_new_block() {
    let mut storage_reader = MockBatcherStorageReader::new();
    storage_reader
        .expect_state_diff_height()
        .returning(|| Ok(FIRST_BLOCK_NUMBER_WITH_PARTIAL_BLOCK_HASH));
    storage_reader
        .expect_global_root_height()
        .returning(|| Ok(FIRST_BLOCK_NUMBER_WITH_PARTIAL_BLOCK_HASH));
    let mut mock_dependencies = MockDependencies { storage_reader, ..Default::default() };

    // Expect setting the block hash for the last old block (i.e the parent of the first new block).
    mock_dependencies
        .storage_writer
        .expect_set_block_hash()
        .times(1)
        .with(eq(FIRST_BLOCK_NUMBER_WITH_PARTIAL_BLOCK_HASH.prev().unwrap()), eq(DUMMY_BLOCK_HASH))
        .returning(|_, _| Ok(()));
    mock_dependencies
        .storage_writer
        .expect_commit_proposal()
        .times(1)
        .with(
            eq(FIRST_BLOCK_NUMBER_WITH_PARTIAL_BLOCK_HASH),
            eq(ThinStateDiff::default()),
            eq(StorageCommitmentBlockHash::Partial(PartialBlockHashComponents {
                block_number: FIRST_BLOCK_NUMBER_WITH_PARTIAL_BLOCK_HASH,
                ..Default::default()
            })),
        )
        .returning(|_, _, _| Ok(()));

    mock_dependencies
        .clients
        .l1_provider_client
        .expect_commit_block()
        .times(1)
        .with(
            eq(IndexSet::new()),
            eq(IndexSet::new()),
            eq(FIRST_BLOCK_NUMBER_WITH_PARTIAL_BLOCK_HASH),
        )
        .returning(|_, _, _| Ok(()));

    let mut batcher = create_batcher(mock_dependencies).await;

    let sync_block = SyncBlock {
        block_header_without_hash: BlockHeaderWithoutHash {
            block_number: FIRST_BLOCK_NUMBER_WITH_PARTIAL_BLOCK_HASH,
            starknet_version: StarknetVersion::LATEST,
            parent_hash: DUMMY_BLOCK_HASH,
            ..Default::default()
        },
        block_header_commitments: Some(Default::default()),
        ..Default::default()
    };
    batcher.add_sync_block(sync_block).await.unwrap();
}

#[rstest]
#[tokio::test]
#[should_panic(expected = "does not match the configured parent block hash")]
async fn add_sync_block_parent_hash_mismatch() {
    let mut storage_reader = MockBatcherStorageReader::new();
    storage_reader
        .expect_state_diff_height()
        .returning(|| Ok(FIRST_BLOCK_NUMBER_WITH_PARTIAL_BLOCK_HASH));
    storage_reader
        .expect_global_root_height()
        .returning(|| Ok(FIRST_BLOCK_NUMBER_WITH_PARTIAL_BLOCK_HASH));
    let mock_dependencies = MockDependencies { storage_reader, ..Default::default() };

    let mut batcher = create_batcher(mock_dependencies).await;

    // Provide a parent_hash that doesn't match the configured DUMMY_BLOCK_HASH.
    let wrong_parent_hash = BlockHash(Felt::from_hex_unchecked("0xbadbeef"));
    let sync_block = SyncBlock {
        block_header_without_hash: BlockHeaderWithoutHash {
            block_number: FIRST_BLOCK_NUMBER_WITH_PARTIAL_BLOCK_HASH,
            starknet_version: StarknetVersion::LATEST,
            parent_hash: wrong_parent_hash,
            ..Default::default()
        },
        block_header_commitments: Some(Default::default()),
        ..Default::default()
    };
    let _ = batcher.add_sync_block(sync_block).await;
}

#[rstest]
#[tokio::test]
#[should_panic(expected = "is a new block but is older than the configured first block with \
                           partial block hash components")]
async fn add_sync_block_with_partial_block_hash_but_older_than_configured_first_block() {
    let mut storage_reader = MockBatcherStorageReader::new();
    storage_reader
        .expect_state_diff_height()
        .returning(|| Ok(FIRST_BLOCK_NUMBER_WITH_PARTIAL_BLOCK_HASH.prev().unwrap()));
    storage_reader
        .expect_global_root_height()
        .returning(|| Ok(FIRST_BLOCK_NUMBER_WITH_PARTIAL_BLOCK_HASH.prev().unwrap()));
    let mock_dependencies = MockDependencies { storage_reader, ..Default::default() };
    let mut batcher = create_batcher(mock_dependencies).await;

    let sync_block = SyncBlock {
        block_header_without_hash: BlockHeaderWithoutHash {
            block_number: FIRST_BLOCK_NUMBER_WITH_PARTIAL_BLOCK_HASH.prev().unwrap(),
            starknet_version: StarknetVersion::LATEST,
            ..Default::default()
        },
        block_header_commitments: Some(Default::default()),
        ..Default::default()
    };
    let _ = batcher.add_sync_block(sync_block).await;
}

#[tokio::test]
async fn revert_block() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);

    let mut storage_writer = MockBatcherStorageWriter::new();
    storage_writer
        .expect_revert_block()
        .times(1)
        .with(eq(LATEST_BLOCK_IN_STORAGE))
        .returning(|_| ());

    let storage_reader = mock_storage_reader_for_revert();
    let mock_dependencies =
        MockDependencies { storage_reader, storage_writer, ..Default::default() };

    let committer_offset = mock_dependencies.clients.committer_client.get_offset();

    let mut batcher = create_batcher(mock_dependencies).await;

    let metrics = recorder.handle().render();
    assert_eq!(BUILDING_HEIGHT.parse_numeric_metric::<u64>(&metrics), Some(INITIAL_HEIGHT.0));

    let revert_input = RevertBlockInput { height: LATEST_BLOCK_IN_STORAGE };

    assert_eq!(*(committer_offset.lock().await), INITIAL_HEIGHT);
    batcher.revert_block(revert_input).await.unwrap();
    assert_eq!(*committer_offset.lock().await, LATEST_BLOCK_IN_STORAGE);

    let metrics = recorder.handle().render();
    assert_eq!(BUILDING_HEIGHT.parse_numeric_metric::<u64>(&metrics), Some(INITIAL_HEIGHT.0 - 1));
    assert_eq!(REVERTED_BLOCKS.parse_numeric_metric::<usize>(&metrics), Some(1));
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
    let mut storage_reader = MockBatcherStorageReader::new();
    storage_reader.expect_state_diff_height().returning(|| Ok(BlockNumber(0)));
    storage_reader.expect_global_root_height().returning(|| Ok(BlockNumber(0)));
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
    let expected_artifacts = BlockExecutionArtifacts::create_for_testing().await;

    mock_dependencies
        .clients
        .mempool_client
        .expect_commit_block()
        .times(1)
        .with(eq(CommitBlockArgs {
            address_to_nonce: expected_artifacts.address_to_nonce(),
            rejected_tx_hashes: expected_artifacts.execution_data.rejected_tx_hashes.clone(),
        }))
        .returning(|_| Ok(()));

    mock_dependencies
        .clients
        .l1_provider_client
        .expect_start_block()
        .times(1)
        .with(eq(SessionState::Propose), eq(INITIAL_HEIGHT))
        .returning(|_, _| Ok(()));

    mock_dependencies
        .clients
        .l1_provider_client
        .expect_commit_block()
        .times(1)
        .with(eq(IndexSet::new()), eq(IndexSet::new()), eq(INITIAL_HEIGHT))
        .returning(|_, _, _| Ok(()));

    let expected_partial_block_hash = expected_artifacts.partial_block_hash_components();
    mock_dependencies
        .storage_writer
        .expect_commit_proposal()
        .times(1)
        .with(
            eq(INITIAL_HEIGHT),
            eq(expected_artifacts.thin_state_diff()),
            eq(StorageCommitmentBlockHash::Partial(expected_partial_block_hash)),
        )
        .returning(|_, _, _| Ok(()));

    mock_dependencies
        .storage_reader
        .expect_get_parent_hash_and_partial_block_hash_components()
        .with(eq(INITIAL_HEIGHT.prev().unwrap()))
        .returning(|_| {
            Ok((Some(BlockHash::default()), Some(PartialBlockHashComponents::default())))
        });

    mock_create_builder_for_propose_block(
        &mut mock_dependencies.clients.block_builder_factory,
        vec![],
        Ok(BlockExecutionArtifacts::create_for_testing().await),
    );

    let decision_reached_response =
        batcher_propose_and_commit_block(mock_dependencies).await.unwrap();

    verify_decision_reached_response(&decision_reached_response, &expected_artifacts);

    let metrics = recorder.handle().render();
    assert_eq!(
        BUILDING_HEIGHT.parse_numeric_metric::<u64>(&metrics),
        Some(INITIAL_HEIGHT.unchecked_next().0)
    );
    assert_eq!(
        BATCHED_TRANSACTIONS.parse_numeric_metric::<usize>(&metrics),
        Some(expected_artifacts.execution_data.execution_infos_and_signatures.len())
    );
    assert_eq!(
        REJECTED_TRANSACTIONS.parse_numeric_metric::<usize>(&metrics),
        Some(expected_artifacts.execution_data.rejected_tx_hashes.len())
    );
    assert_eq!(
        REVERTED_TRANSACTIONS.parse_numeric_metric::<usize>(&metrics),
        Some(
            expected_artifacts
                .execution_data
                .execution_infos_and_signatures
                .values()
                .filter(|(info, _)| info.revert_error.is_some())
                .count(),
        )
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
    mock_dependencies.clients.l1_provider_client.expect_start_block().returning(|_, _| Ok(()));
    mock_dependencies.clients.mempool_client.expect_commit_block().returning(|_| Ok(()));
    mock_dependencies.clients.l1_provider_client.expect_commit_block().returning(|_, _, _| Ok(()));
    mock_dependencies.storage_writer.expect_commit_proposal().returning(|_, _, _| Ok(()));

    let block_builder_result = BlockExecutionArtifacts::create_for_testing().await;
    // Check that the execution_infos were initiated properly for this test.
    let execution_infos = block_builder_result
        .execution_data
        .execution_infos_and_signatures
        .iter()
        .map(|(hash, (info, _))| (*hash, info.clone()))
        .collect();
    verify_indexed_execution_infos(&execution_infos);

    mock_dependencies
        .storage_reader
        .expect_get_parent_hash_and_partial_block_hash_components()
        .with(eq(INITIAL_HEIGHT.prev().unwrap()))
        .returning(|_| {
            Ok((Some(BlockHash::default()), Some(PartialBlockHashComponents::default())))
        });

    mock_create_builder_for_propose_block(
        &mut mock_dependencies.clients.block_builder_factory,
        vec![],
        Ok(block_builder_result),
    );

    let decision_reached_response =
        batcher_propose_and_commit_block(mock_dependencies).await.unwrap();

    // Verify that the execution_infos are in the same order as returned from the block_builder.
    assert_eq!(decision_reached_response.central_objects.execution_infos, execution_infos);
}

#[tokio::test]
async fn mempool_not_ready() {
    let mut mock_dependencies = MockDependencies::default();
    mock_dependencies.clients.mempool_client.checkpoint();
    mock_dependencies.clients.mempool_client.expect_update_gas_price().returning(|_| {
        Err(MempoolClientError::ClientError(ClientError::CommunicationFailure("".to_string())))
    });
    mock_dependencies
        .clients
        .mempool_client
        .expect_commit_block()
        .with(eq(CommitBlockArgs::default()))
        .returning(|_| Ok(()));
    mock_dependencies.clients.l1_provider_client.expect_start_block().returning(|_, _| Ok(()));

    let mut batcher = create_batcher(mock_dependencies).await;
    batcher.start_height(StartHeightInput { height: INITIAL_HEIGHT }).await.unwrap();
    let result = batcher.propose_block(propose_block_input(PROPOSAL_ID)).await;
    assert_eq!(result, Err(BatcherError::InternalError));
}

#[test]
fn validate_batcher_config_failure() {
    let config = BatcherConfig {
        static_config: BatcherStaticConfig {
            input_stream_content_buffer_size: 99,
            block_builder_config: BlockBuilderConfig {
                n_concurrent_txs: 100,
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    };

    let error = config.validate().unwrap_err();
    assert!(
        error
            .to_string()
            .contains("input_stream_content_buffer_size must be at least n_concurrent_txs")
    );
}

#[rstest]
#[case::communication_failure(
    L1EventsProviderClientError::ClientError(ClientError::CommunicationFailure("L1 commit failed".to_string()))
)]
#[case::unexpected_height(
    L1EventsProviderClientError::L1EventsProviderError(L1EventsProviderError::UnexpectedHeight {
        expected_height: INITIAL_HEIGHT,
        got: INITIAL_HEIGHT,
    })
)]
#[tokio::test]
async fn decision_reached_return_success_when_l1_commit_block_fails(
    #[case] l1_error: L1EventsProviderClientError,
) {
    let mut mock_dependencies = MockDependencies::default();

    mock_dependencies.clients.l1_provider_client.expect_start_block().returning(|_, _| Ok(()));

    mock_dependencies
        .clients
        .l1_provider_client
        .expect_commit_block()
        .times(1)
        .returning(move |_, _, _| Err(l1_error.clone()));

    mock_dependencies.storage_writer.expect_commit_proposal().returning(|_, _, _| Ok(()));

    mock_dependencies.clients.mempool_client.expect_commit_block().returning(|_| Ok(()));

    mock_dependencies
        .storage_reader
        .expect_get_parent_hash_and_partial_block_hash_components()
        .with(eq(INITIAL_HEIGHT.prev().unwrap()))
        .returning(|_| {
            Ok((Some(BlockHash::default()), Some(PartialBlockHashComponents::default())))
        });

    mock_create_builder_for_propose_block(
        &mut mock_dependencies.clients.block_builder_factory,
        vec![],
        Ok(BlockExecutionArtifacts::create_for_testing().await),
    );

    let result = batcher_propose_and_commit_block(mock_dependencies).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn get_height_with_real_storage() {
    // Real storage starts at height 0.
    let batcher =
        create_batcher_with_real_storage(MockDependenciesWithRealStorage::default()).await;

    let result = batcher.get_height().await;
    assert_eq!(result, Ok(GetHeightResponse { height: BlockNumber(0) }));
}

#[tokio::test]
async fn set_and_get_block_hash_with_real_storage() {
    let mut batcher =
        create_batcher_with_real_storage(MockDependenciesWithRealStorage::default()).await;
    let height = BlockNumber(42);
    let block_hash = BlockHash(12345_u32.into());

    batcher.storage_writer.set_block_hash(height, block_hash).unwrap();
    // Check the set of block hash.
    assert_eq!(batcher.storage_reader.get_block_hash(height).unwrap(), Some(block_hash));
    // Check unset block hash.
    assert_eq!(batcher.storage_reader.get_block_hash(height.unchecked_next()).unwrap(), None);
}

#[tokio::test]
async fn get_block_hash() {
    let mut mock_dependencies = MockDependencies::default();
    mock_dependencies
        .storage_reader
        .expect_get_block_hash()
        .with(eq(INITIAL_HEIGHT))
        .returning(|_| Ok(Some(BlockHash::default())));

    let mut batcher = create_batcher(mock_dependencies).await;
    let result = batcher.get_block_hash(INITIAL_HEIGHT);
    assert_eq!(result, Ok(BlockHash::default()));
}

#[tokio::test]
async fn get_block_hash_not_found() {
    let mut mock_dependencies = MockDependencies::default();
    mock_dependencies
        .storage_reader
        .expect_get_block_hash()
        .with(eq(INITIAL_HEIGHT))
        .returning(|_| Ok(None));
    let mut batcher = create_batcher(mock_dependencies).await;
    let result = batcher.get_block_hash(INITIAL_HEIGHT);
    assert_eq!(result, Err(BatcherError::BlockHashNotFound(INITIAL_HEIGHT)));
}

#[tokio::test]
async fn get_block_hash_after_reading_commitment_results() {
    let mut mock_dependencies = MockDependencies::default();
    let global_root = GlobalRoot::default();
    let partial_components =
        PartialBlockHashComponents { block_number: INITIAL_HEIGHT, ..Default::default() };
    let parent_hash = BlockHash::default();
    let expected_block_hash =
        calculate_block_hash(&partial_components, global_root, parent_hash).unwrap();

    // Should be called by the commitment manager when finalizing results and writing them to
    // storage.
    mock_dependencies
        .storage_reader
        .expect_get_parent_hash_and_partial_block_hash_components()
        .with(eq(INITIAL_HEIGHT))
        .returning(move |_| Ok((Some(parent_hash), Some(partial_components.clone()))));
    mock_dependencies
        .storage_writer
        .expect_set_global_root_and_block_hash()
        .times(1)
        .with(eq(INITIAL_HEIGHT), eq(global_root), always())
        .returning(|_, _, _| Ok(()));

    let mut batcher = create_batcher(mock_dependencies).await;

    // Send a commitment task directly to the state committer so a result will be available.
    let task = CommitterTaskInput::Commit(CommitBlockRequest {
        height: INITIAL_HEIGHT,
        state_diff: ThinStateDiff::default(),
        state_diff_commitment: None,
    });
    batcher.commitment_manager.tasks_sender.send(task).await.unwrap();
    wait_for_n_items(&mut batcher.commitment_manager.results_receiver, 1).await;

    let result = batcher.get_block_hash(INITIAL_HEIGHT);
    assert_eq!(result, Ok(expected_block_hash));
    assert_eq!(
        get_number_of_items_in_channel_from_receiver(&batcher.commitment_manager.results_receiver),
        0
    );
}

#[tokio::test]
async fn get_block_hash_error() {
    let mut mock_dependencies = MockDependencies::default();
    mock_dependencies
        .storage_reader
        .expect_get_block_hash()
        .with(eq(INITIAL_HEIGHT))
        .returning(|_| Err(StorageError::InnerError(DbError::InnerDeserialization)));
    let mut batcher = create_batcher(mock_dependencies).await;
    let result = batcher.get_block_hash(INITIAL_HEIGHT);
    assert_eq!(result, Err(BatcherError::InternalError));
}

/// For every key in the original map, validates that the reversed map values are identical to the
/// base map, or zero if the key is missing in the base map.
fn validate_is_reversed<K: Eq + Hash + Debug, V: Debug + Default + Eq + Hash>(
    base: IndexMap<K, V>,
    original: IndexMap<K, V>,
    reversed: IndexMap<K, V>,
) {
    assert_eq!(original.len(), reversed.len());
    for key in original.keys() {
        assert_eq!(reversed.get(key).unwrap(), base.get(key).unwrap_or(&V::default()));
    }
}

#[tokio::test]
async fn test_reversed_state_diff() {
    let mut batcher =
        create_batcher_with_real_storage(MockDependenciesWithRealStorage::default()).await;

    let state_diffs = get_overlapping_state_diffs(2);

    let mut height = BlockNumber(0);
    let base_state_diff = state_diffs[0].clone();
    write_state_diff(&mut batcher, height, &base_state_diff);

    height = height.unchecked_next();
    let original_state_diff = state_diffs[1].clone();
    write_state_diff(&mut batcher, height, &original_state_diff);

    let reversed_state_diff = batcher.storage_reader.reversed_state_diff(height).unwrap();

    validate_is_reversed(
        base_state_diff.deployed_contracts,
        original_state_diff.deployed_contracts,
        reversed_state_diff.deployed_contracts,
    );
    for (contract_address, storage_diffs) in original_state_diff.storage_diffs {
        validate_is_reversed(
            base_state_diff
                .storage_diffs
                .get(&contract_address)
                .unwrap_or(&IndexMap::new())
                .clone(),
            storage_diffs,
            reversed_state_diff.storage_diffs.get(&contract_address).unwrap().clone(),
        );
    }
    validate_is_reversed(
        base_state_diff.class_hash_to_compiled_class_hash,
        original_state_diff.class_hash_to_compiled_class_hash.clone(),
        reversed_state_diff.class_hash_to_compiled_class_hash,
    );
    validate_is_reversed(
        base_state_diff.nonces,
        original_state_diff.nonces.clone(),
        reversed_state_diff.nonces,
    );
}

fn validation_only_mock_dependencies() -> MockDependencies {
    let mut deps = MockDependencies::default();
    deps.batcher_config.static_config.validation_only = true;
    deps
}

#[tokio::test]
async fn validation_only_propose_block_returns_not_supported() {
    let mut batcher = create_batcher(validation_only_mock_dependencies()).await;
    batcher.start_height(StartHeightInput { height: INITIAL_HEIGHT }).await.unwrap();

    let result = batcher.propose_block(propose_block_input(PROPOSAL_ID)).await;

    assert_eq!(result, Err(BatcherError::ProposingNotSupported));
}

#[tokio::test]
#[should_panic(expected = "Mempool client must be present in non-validation-only mode.")]
async fn validation_only_get_batch_timestamp_panics() {
    let batcher = create_batcher(validation_only_mock_dependencies()).await;
    batcher.get_batch_timestamp().await.unwrap();
}

#[tokio::test]
async fn validation_only_validate_block_succeeds() {
    let mut mock_deps = validation_only_mock_dependencies();
    let mut block_builder_factory = MockBlockBuilderFactoryTrait::new();
    mock_create_builder_for_validate_block(
        &mut block_builder_factory,
        Ok(BlockExecutionArtifacts::create_for_testing().await),
    );
    mock_deps.clients.block_builder_factory = block_builder_factory;
    mock_deps.clients.l1_provider_client.expect_start_block().returning(|_, _| Ok(()));

    let mut batcher = create_batcher(mock_deps).await;
    batcher.start_height(StartHeightInput { height: INITIAL_HEIGHT }).await.unwrap();

    batcher.validate_block(validate_block_input(PROPOSAL_ID)).await.unwrap();

    let finish_proposal = FinishProposalInput {
        proposal_id: PROPOSAL_ID,
        final_n_executed_txs: DUMMY_FINAL_N_EXECUTED_TXS,
    };
    let result = batcher.finish_proposal(finish_proposal).await.unwrap();
    assert_matches!(result, FinishProposalStatus::Finished(_));
}

#[tokio::test]
async fn validation_only_decision_reached_skips_mempool_notification() {
    let mut mock_deps = validation_only_mock_dependencies();

    // The mempool_client on MockClients still exists but must not be called.
    mock_deps.clients.mempool_client.checkpoint();

    mock_deps.clients.l1_provider_client.expect_start_block().returning(|_, _| Ok(()));
    mock_deps.clients.l1_provider_client.expect_commit_block().times(1).returning(|_, _, _| Ok(()));
    mock_deps.storage_writer.expect_commit_proposal().returning(|_, _, _| Ok(()));

    let mut block_builder_factory = MockBlockBuilderFactoryTrait::new();
    mock_create_builder_for_validate_block(
        &mut block_builder_factory,
        Ok(BlockExecutionArtifacts::create_for_testing().await),
    );
    mock_deps.clients.block_builder_factory = block_builder_factory;

    let mut batcher = create_batcher(mock_deps).await;
    batcher.start_height(StartHeightInput { height: INITIAL_HEIGHT }).await.unwrap();
    batcher.validate_block(validate_block_input(PROPOSAL_ID)).await.unwrap();
    batcher.await_active_proposal(DUMMY_FINAL_N_EXECUTED_TXS).await.unwrap();

    // decision_reached must succeed and not call mempool_client.commit_block.
    batcher.decision_reached(DecisionReachedInput { proposal_id: PROPOSAL_ID }).await.unwrap();
}

#[tokio::test]
#[should_panic(expected = "validation_only=false but mempool_client is None")]
async fn validation_only_flag_false_with_no_mempool_panics() {
    new_batcher_with_mempool_override(MockDependencies::default(), None).await;
}

#[tokio::test]
#[should_panic(expected = "validation_only=true but mempool_client is Some")]
async fn validation_only_flag_true_with_mempool_panics() {
    let mempool: Option<SharedMempoolClient> = Some(Arc::new(MockMempoolClient::new()));
    new_batcher_with_mempool_override(validation_only_mock_dependencies(), mempool).await;
}
