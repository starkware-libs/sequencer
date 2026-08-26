use std::fmt::Debug;
use std::hash::Hash;
use std::sync::mpsc::{channel, Receiver};
use std::sync::{mpsc, Arc, Mutex};
use std::task::Poll;
use std::time::Duration;

use apollo_batcher_config::config::{
    BatcherConfig,
    BatcherDynamicConfig,
    BatcherStaticConfig,
    StateCommitmentInfosPruningConfig,
};
use apollo_batcher_types::batcher_types::{
    CallContractInput,
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
    PruneStateCommitmentInfosInput,
    RevertBlockInput,
    SendTxsForProposalInput,
    SendTxsForProposalStatus,
    StartHeightInput,
    ValidateBlockInput,
};
use apollo_batcher_types::errors::BatcherError;
use apollo_class_manager_types::MockClassManagerClient;
use apollo_committer_types::committer_types::CommitBlockRequest;
use apollo_config_manager_types::communication::MockConfigManagerClient;
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
use apollo_storage::accessed_keys::AccessedKeys;
use apollo_storage::db::DbError;
use apollo_storage::partial_block_hash::PartialBlockHashComponentsStorageWriter;
use apollo_storage::state::StateStorageWriter;
use apollo_storage::state_commitment_infos::PrunedStateCommitmentInfosPointers;
use apollo_storage::test_utils::get_test_storage;
use apollo_storage::{StorageError, StorageReader, StorageWriter};
use assert_matches::assert_matches;
use blockifier::abi::constants;
use blockifier::blockifier::config::{ContractClassManagerConfig, NativeClassesWhitelist};
use blockifier::context::{BlockContext, ChainInfo};
use blockifier::execution::contract_class::RunnableCompiledClass;
use blockifier::state::cached_state::CachedState;
use blockifier::state::contract_class_manager::ContractClassManager;
use blockifier::state::errors::StateError;
use blockifier::state::state_api::{StateReader, StateResult};
use blockifier::test_utils::contracts::FeatureContractTrait;
use blockifier::test_utils::dict_state_reader::DictStateReader;
use blockifier::test_utils::initial_test_state::test_state;
use blockifier::test_utils::BALANCE;
use blockifier::transaction::test_utils::{
    default_all_resource_bounds,
    invoke_tx_with_default_flags,
};
use blockifier::transaction::transactions::ExecutableTransaction;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::calldata::create_calldata;
use blockifier_test_utils::contracts::FeatureContract;
use futures::poll;
use indexmap::{indexmap, IndexMap, IndexSet};
use metrics_exporter_prometheus::PrometheusBuilder;
use mockall::predicate::{always, eq};
use rstest::rstest;
use starknet_api::block::{
    BlockHash,
    BlockHeaderWithoutHash,
    BlockInfo,
    BlockNumber,
    BlockTimestamp,
    GasPrice,
    GasPricePerToken,
    StarknetVersion,
};
use starknet_api::block_hash::block_hash_calculator::{
    calculate_block_hash,
    concat_counts,
    BlockHeaderCommitments,
    PartialBlockHash,
    PartialBlockHashComponents,
};
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::contract_class::ContractClass;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, GlobalRoot, Nonce};
use starknet_api::data_availability::L1DataAvailabilityMode;
use starknet_api::state::{SierraContractClass, StorageKey, ThinStateDiff};
use starknet_api::transaction::TransactionHash;
use starknet_api::{class_hash, invoke_tx_args, tx_hash};
use starknet_types_core::felt::Felt;
use tempfile::TempDir;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use validator::Validate;

use crate::batcher::{
    finished_proposal_info_from_artifacts,
    validate_retdata_length,
    Batcher,
    BatcherStorageReader,
    BatcherStorageWriter,
    MockBatcherStorageReader,
    MockBatcherStorageWriter,
    StorageCommitmentBlockHash,
    StorageViewStateReaderFactory,
    ViewStateReaderFactory,
    MAX_CONCURRENT_VIEW_CALLS,
    MAX_VIEW_CALL_RETDATA_LENGTH,
    TOO_MANY_VIEW_CALLS_REASON,
};
use crate::block_builder::{
    AbortSignalSender,
    BlockBuilderError,
    BlockBuilderResult,
    BlockExecutionArtifacts,
    MockBlockBuilderFactoryTrait,
};
use crate::commitment_manager::commitment_manager_impl::CommitmentManager;
use crate::commitment_manager::types::{CommitterTaskInput, CommitterTaskOutput};
use crate::metrics::{
    BATCHED_TRANSACTIONS,
    BUILDING_HEIGHT,
    LAST_SYNCED_BLOCK_HEIGHT,
    PROPOSAL_ABORTED,
    PROPOSAL_FAILED,
    PROPOSAL_STARTED,
    PROPOSAL_SUCCEEDED,
    REJECTED_TRANSACTIONS,
    REJECTED_VIEW_CALLS,
    REVERTED_BLOCKS,
    REVERTED_TRANSACTIONS,
    STATE_COMMITMENT_INFOS_LOWER_BOUND,
    SYNCED_TRANSACTIONS,
};
use crate::test_utils::{
    get_number_of_items_in_channel_from_receiver,
    mock_storage_reader,
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
    STATE_COMMITMENT_INFOS_LOWER_BOUND_HEIGHT,
    STREAMING_CHUNK_SIZE,
};

const STAKING_CONTRACT: FeatureContract =
    FeatureContract::MockStakingContract(RunnableCairo1::Casm);
const ACCOUNT_CONTRACT: FeatureContract =
    FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1(RunnableCairo1::Casm));
/// Both versions of the test contract expose a `recurse(depth)` entry point whose cost grows with
/// the requested depth. The Cairo 1 contract is tracked by Sierra gas and the Cairo 0 one by Cairo
/// steps, so between them they exercise both of the view call resource bounds.
const SIERRA_GAS_TRACKED_RECURSIVE_CONTRACT: FeatureContract =
    FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));
const CAIRO_STEPS_TRACKED_RECURSIVE_CONTRACT: FeatureContract =
    FeatureContract::TestContract(CairoVersion::Cairo0);
/// Recursion depth whose cost is a small fraction of either view call resource bound.
const RECURSION_DEPTH_WITHIN_RESOURCE_BOUNDS: u64 = 1_000;
/// Recursion depths that exhaust a view call resource bound: above `VIEW_CALL_MAX_SIERRA_GAS` for
/// the Cairo 1 contract, and between `VIEW_CALL_MAX_N_STEPS` and the block's
/// `invoke_tx_max_n_steps` (10^7) for the Cairo 0 one, so the step case fails on the view bound and
/// not the block limit. One level of `recurse` costs about 973 Sierra gas and about 4 Cairo steps.
const SIERRA_GAS_RECURSION_DEPTH_EXCEEDING_RESOURCE_BOUNDS: u64 = 300_000;
const CAIRO_STEPS_RECURSION_DEPTH_EXCEEDING_RESOURCE_BOUNDS: u64 = 400_000;
/// Height the view call tests write their state diff at. A view call reads at
/// `state_diff_height()`, one past the last written diff, and takes its block info from the block
/// before that.
const LAST_COMMITTED_HEIGHT: BlockNumber = BlockNumber(0);
/// Distinguishable from the default, so a test can tell a real committed value from a synthesized
/// one.
const COMMITTED_BLOCK_TIMESTAMP: BlockTimestamp = BlockTimestamp(1_700_000_000);

struct TestViewStateReaderFactory {
    state: Arc<Mutex<CachedState<DictStateReader>>>,
    expected_block_number: BlockNumber,
}

impl ViewStateReaderFactory for TestViewStateReaderFactory {
    fn create(
        &self,
        block_number: BlockNumber,
        _native_classes_whitelist: NativeClassesWhitelist,
        _runtime: tokio::runtime::Handle,
        _class_manager_request_timeout: Duration,
    ) -> Box<dyn StateReader + Send> {
        assert_eq!(block_number, self.expected_block_number);
        Box::new(self.state.lock().unwrap().clone())
    }
}

/// Creates readers that park the blocking thread executing the view call until released,
/// reproducing a view call that outlives the caller waiting for it. Serves a single view call.
struct ParkedViewStateReaderFactory {
    entered_sender: UnboundedSender<()>,
    release_receiver: Mutex<Option<mpsc::Receiver<()>>>,
}

impl ViewStateReaderFactory for ParkedViewStateReaderFactory {
    fn create(
        &self,
        _block_number: BlockNumber,
        _native_classes_whitelist: NativeClassesWhitelist,
        _runtime: tokio::runtime::Handle,
        _class_manager_request_timeout: Duration,
    ) -> Box<dyn StateReader + Send> {
        let release_receiver =
            self.release_receiver.lock().unwrap().take().expect("Expected a single view call.");
        Box::new(ParkedViewStateReader {
            entered_sender: self.entered_sender.clone(),
            release_receiver: Mutex::new(Some(release_receiver)),
            state: DictStateReader::default(),
        })
    }
}

/// An empty state whose first access parks the calling thread until released.
struct ParkedViewStateReader {
    entered_sender: UnboundedSender<()>,
    /// Taken by the first state access, so that only that access parks.
    release_receiver: Mutex<Option<mpsc::Receiver<()>>>,
    state: DictStateReader,
}

impl ParkedViewStateReader {
    fn park_on_first_access(&self) {
        let release_receiver = self.release_receiver.lock().unwrap().take();
        if let Some(release_receiver) = release_receiver {
            self.entered_sender.send(()).unwrap();
            release_receiver.recv().unwrap();
        }
    }
}

impl StateReader for ParkedViewStateReader {
    fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
    ) -> StateResult<Felt> {
        self.park_on_first_access();
        self.state.get_storage_at(contract_address, key)
    }

    fn get_nonce_at(&self, contract_address: ContractAddress) -> StateResult<Nonce> {
        self.park_on_first_access();
        self.state.get_nonce_at(contract_address)
    }

    fn get_class_hash_at(&self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        self.park_on_first_access();
        self.state.get_class_hash_at(contract_address)
    }

    fn get_compiled_class(&self, class_hash: ClassHash) -> StateResult<RunnableCompiledClass> {
        self.park_on_first_access();
        self.state.get_compiled_class(class_hash)
    }

    fn get_compiled_class_hash(&self, class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        self.park_on_first_access();
        self.state.get_compiled_class_hash(class_hash)
    }
}

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

/// Expects a single `commit_proposal` call with the given arguments; `expect_accessed_keys`
/// states whether accessed keys should be written with the state diff.
fn expect_commit_proposal_once(
    storage_writer: &mut MockBatcherStorageWriter,
    expected_height: BlockNumber,
    expected_state_diff: ThinStateDiff,
    expected_storage_commitment_block_hash: StorageCommitmentBlockHash,
    expect_accessed_keys: bool,
) {
    storage_writer
        .expect_commit_proposal()
        .times(1)
        .withf(move |height, state_diff, storage_commitment_block_hash, accessed_keys| {
            *height == expected_height
                && *state_diff == expected_state_diff
                && *storage_commitment_block_hash == expected_storage_commitment_block_hash
                && accessed_keys.is_some() == expect_accessed_keys
        })
        .returning(|_, _, _, _| Ok(()));
}

fn expect_commit_proposal_success(storage_writer: &mut MockBatcherStorageWriter) {
    storage_writer.expect_commit_proposal().returning(|_, _, _, _| Ok(()));
}

fn write_state_diff(batcher: &mut Batcher, height: BlockNumber, state_diff: &ThinStateDiff) {
    batcher
        .storage_writer
        .commit_proposal(
            height,
            state_diff.clone(),
            StorageCommitmentBlockHash::Partial(PartialBlockHashComponents::default()),
            None,
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
    class_manager_client: MockClassManagerClient,
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
            class_manager_client: MockClassManagerClient::new(),
            batcher_config: BatcherConfig {
                static_config: BatcherStaticConfig {
                    outstream_content_buffer_size: STREAMING_CHUNK_SIZE,
                    ..Default::default()
                },
                // Compiling a class in a debug build takes longer than the production timeout.
                dynamic_config: BatcherDynamicConfig {
                    view_call_timeout_millis: Duration::from_secs(300),
                    ..Default::default()
                },
            },
            _temp_dir: temp_dir,
        }
    }
}

async fn create_batcher(mock_dependencies: MockDependencies) -> Batcher {
    create_batcher_impl(
        Arc::new(mock_dependencies.storage_reader),
        mock_dependencies.view_state_reader_factory,
        Box::new(mock_dependencies.storage_writer),
        mock_dependencies.clients,
        mock_dependencies.batcher_config,
    )
    .await
}

async fn create_batcher_with_real_storage(
    mock_dependencies: MockDependenciesWithRealStorage,
) -> Batcher {
    let view_state_reader_factory = Box::new(StorageViewStateReaderFactory {
        storage_reader: mock_dependencies.storage_reader.clone(),
        contract_class_manager: ContractClassManager::start(ContractClassManagerConfig::default()),
        class_manager_client: Arc::new(mock_dependencies.class_manager_client),
    });
    create_batcher_impl(
        Arc::new(mock_dependencies.storage_reader),
        view_state_reader_factory,
        Box::new(mock_dependencies.storage_writer),
        mock_dependencies.clients,
        mock_dependencies.batcher_config,
    )
    .await
}

async fn create_batcher_impl<R: BatcherStorageReader + 'static>(
    storage_reader: Arc<R>,
    view_state_reader_factory: Box<dyn ViewStateReaderFactory>,
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

    let mut mock_config_manager = MockConfigManagerClient::new();
    mock_config_manager
        .expect_get_batcher_dynamic_config()
        .returning(|| Ok(BatcherDynamicConfig::default()));

    let mut batcher = Batcher::new(
        config,
        storage_reader,
        view_state_reader_factory,
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
    let mut mock_config_manager = MockConfigManagerClient::new();
    mock_config_manager
        .expect_get_batcher_dynamic_config()
        .returning(|| Ok(BatcherDynamicConfig::default()));
    Batcher::new(
        deps.batcher_config,
        storage_reader,
        deps.view_state_reader_factory,
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
        |_, _, _, tx_provider, _, _, _| {
            let block_builder = FakeValidateBlockBuilder {
                tx_provider,
                build_block_result: Some(build_block_result),
            };
            Ok((Box::new(block_builder), abort_signal_sender()))
        },
    );
}

fn mock_storage_reader_for_revert() -> MockBatcherStorageReader {
    let mut storage_reader = mock_storage_reader();
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
        move |_, _, _, tx_provider, output_content_sender, _, _| {
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
    assert_eq!(
        STATE_COMMITMENT_INFOS_LOWER_BOUND.parse_numeric_metric::<u64>(&metrics),
        Some(STATE_COMMITMENT_INFOS_LOWER_BOUND_HEIGHT.0)
    );
}

/// A storage with no state commitment infos retains none below its height, so the bound the
/// metric reports is that height.
#[tokio::test]
async fn state_commitment_infos_lower_bound_metric_defaults_to_the_storage_height() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    let mut storage_reader = MockBatcherStorageReader::new();
    storage_reader.expect_state_diff_height().returning(|| Ok(INITIAL_HEIGHT));
    storage_reader.expect_global_root_height().returning(|| Ok(INITIAL_HEIGHT));
    storage_reader.expect_state_commitment_infos_lower_bound().returning(|| Ok(None));
    let _batcher = create_batcher(MockDependencies { storage_reader, ..Default::default() }).await;

    let metrics = recorder.handle().render();
    assert_eq!(
        STATE_COMMITMENT_INFOS_LOWER_BOUND.parse_numeric_metric::<u64>(&metrics),
        Some(INITIAL_HEIGHT.0)
    );
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
    let mut storage_reader = mock_storage_reader();
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
    let mut storage_reader = mock_storage_reader();
    storage_reader.expect_state_diff_height().returning(|| Ok(INITIAL_HEIGHT));
    storage_reader.expect_global_root_height().returning(|| Ok(INITIAL_HEIGHT));

    let batcher = create_batcher(MockDependencies { storage_reader, ..Default::default() }).await;

    let result = batcher.get_height().await.unwrap();
    assert_eq!(result, GetHeightResponse { height: INITIAL_HEIGHT });
}

#[rstest]
#[tokio::test]
async fn propose_block_without_retrospective_block_hash() {
    let mut storage_reader = mock_storage_reader();
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
}), None)]
#[case::old_sync_block(FIRST_BLOCK_NUMBER_WITH_PARTIAL_BLOCK_HASH.prev().unwrap(), None, None)]
#[case::new_sync_block_with_accessed_keys(INITIAL_HEIGHT, Some(PartialBlockHashComponents {
    block_number: INITIAL_HEIGHT,
    ..Default::default()
}), Some(AccessedKeys::default()))]
#[tokio::test]
async fn add_sync_block(
    #[case] block_number: BlockNumber,
    #[case] partial_block_hash_components: Option<PartialBlockHashComponents>,
    #[case] accessed_keys: Option<AccessedKeys>,
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

    let mut storage_reader = mock_storage_reader();
    storage_reader.expect_state_diff_height().returning(move || Ok(block_number));
    storage_reader.expect_global_root_height().returning(move || Ok(block_number));

    let mut storage_writer = MockBatcherStorageWriter::new();
    expect_commit_proposal_once(
        &mut storage_writer,
        block_number,
        test_state_diff(),
        storage_commitment_block_hash,
        accessed_keys.is_some(),
    );

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
    batcher.add_sync_block(sync_block, accessed_keys.clone()).await.unwrap();

    // Providing accessed keys should issue a `ReadPathsAndCommitBlock` committer task; otherwise a
    // plain `Commit` task is issued.
    wait_for_n_items(&mut batcher.commitment_manager.results_receiver, 1).await;
    let committer_task_output = batcher.commitment_manager.results_receiver.try_recv().unwrap();
    match committer_task_output {
        CommitterTaskOutput::Commit(_) => assert!(accessed_keys.is_none()),
        CommitterTaskOutput::ReadPathsAndCommitBlock(_) => assert!(accessed_keys.is_some()),
        CommitterTaskOutput::Revert(_) => panic!("Unexpected revert committer task."),
    }

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
    let result = batcher.add_sync_block(sync_block, None).await;
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
    let mut storage_reader = mock_storage_reader();
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
    let result = batcher.add_sync_block(sync_block, None).await;
    assert_eq!(result, Err(BatcherError::MissingHeaderCommitments { block_number: INITIAL_HEIGHT }))
}

#[rstest]
#[tokio::test]
#[should_panic(expected = "is at least the first block configured to include a partial hash")]
async fn add_sync_block_missing_block_header_commitments_for_new_block() {
    let block_number = FIRST_BLOCK_NUMBER_WITH_PARTIAL_BLOCK_HASH.unchecked_next();
    let mut storage_reader = mock_storage_reader();
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
    let _ = batcher.add_sync_block(sync_block, None).await;
}

#[rstest]
#[tokio::test]
async fn add_sync_block_for_first_new_block() {
    let mut storage_reader = mock_storage_reader();
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
    expect_commit_proposal_once(
        &mut mock_dependencies.storage_writer,
        FIRST_BLOCK_NUMBER_WITH_PARTIAL_BLOCK_HASH,
        ThinStateDiff::default(),
        StorageCommitmentBlockHash::Partial(PartialBlockHashComponents {
            block_number: FIRST_BLOCK_NUMBER_WITH_PARTIAL_BLOCK_HASH,
            ..Default::default()
        }),
        false,
    );

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
    batcher.add_sync_block(sync_block, None).await.unwrap();
}

#[rstest]
#[tokio::test]
#[should_panic(expected = "does not match the configured parent block hash")]
async fn add_sync_block_parent_hash_mismatch() {
    let mut storage_reader = mock_storage_reader();
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
    let _ = batcher.add_sync_block(sync_block, None).await;
}

#[rstest]
#[tokio::test]
#[should_panic(expected = "is a new block but is older than the configured first block with \
                           partial block hash components")]
async fn add_sync_block_with_partial_block_hash_but_older_than_configured_first_block() {
    let mut storage_reader = mock_storage_reader();
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
    let _ = batcher.add_sync_block(sync_block, None).await;
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
    let mut storage_reader = mock_storage_reader();
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
    expect_commit_proposal_once(
        &mut mock_dependencies.storage_writer,
        INITIAL_HEIGHT,
        expected_artifacts.thin_state_diff(),
        StorageCommitmentBlockHash::Partial(expected_partial_block_hash),
        true,
    );

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
    expect_commit_proposal_success(&mut mock_dependencies.storage_writer);

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
            ..Default::default()
        },
        dynamic_config: BatcherDynamicConfig { n_concurrent_txs: 100, ..Default::default() },
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

    expect_commit_proposal_success(&mut mock_dependencies.storage_writer);

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
    let set_global_root_expectation =
        mock_dependencies.storage_writer.expect_set_global_root_and_block_hash();
    set_global_root_expectation.times(1);
    set_global_root_expectation
        .with(eq(INITIAL_HEIGHT), eq(global_root), always(), always())
        .returning(|_, _, _, _| Ok(()));

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
async fn validation_only_start_round_panics() {
    let mut batcher = create_batcher(validation_only_mock_dependencies()).await;
    batcher.start_round().await.unwrap();
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
    expect_commit_proposal_success(&mut mock_deps.storage_writer);

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

fn undeployed_contract_call_input() -> CallContractInput {
    CallContractInput {
        contract_address: Default::default(),
        entry_point: "get_stakers".to_string(),
        calldata: vec![],
    }
}

/// Creates a batcher whose view calls run against an empty state with no contracts deployed.
async fn create_batcher_with_empty_view_state() -> Batcher {
    create_batcher(MockDependencies {
        view_state_reader_factory: Box::new(TestViewStateReaderFactory {
            state: Arc::new(Mutex::new(CachedState::from(DictStateReader::default()))),
            expected_block_number: INITIAL_HEIGHT,
        }),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn call_contract_contract_not_deployed() {
    let batcher = create_batcher_with_empty_view_state().await;

    let result = batcher.call_contract(undeployed_contract_call_input()).await;

    assert_matches!(result, Err(BatcherError::ContractCallFailed { .. }));
}

#[tokio::test]
async fn call_contract_rejected_when_all_view_call_slots_are_taken() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    let batcher = create_batcher_with_empty_view_state().await;

    let taken_view_call_slots: Vec<_> = (0..MAX_CONCURRENT_VIEW_CALLS)
        .map(|_| batcher.view_call_semaphore.clone().try_acquire_owned().unwrap())
        .collect();

    assert_matches!(
        batcher.call_contract(undeployed_contract_call_input()).await,
        Err(BatcherError::ContractCallFailed { reason }) if reason == TOO_MANY_VIEW_CALLS_REASON,
        "The call should be rejected instead of waiting for a slot."
    );
    assert_eq!(
        REJECTED_VIEW_CALLS.parse_numeric_metric::<u64>(&recorder.handle().render()),
        Some(1)
    );

    // Freeing the slots lets the next call execute; it then fails on the empty state.
    drop(taken_view_call_slots);
    assert_matches!(
        batcher.call_contract(undeployed_contract_call_input()).await,
        Err(BatcherError::ContractCallFailed { reason }) if reason != TOO_MANY_VIEW_CALLS_REASON
    );
}

/// A view call slot models the occupancy of a tokio blocking thread, which no caller-side timeout
/// can cancel. It must therefore outlive a caller that gave up waiting.
#[tokio::test]
async fn view_call_slot_is_freed_only_when_the_blocking_task_ends() {
    let (entered_sender, mut entered_receiver) = unbounded_channel();
    let (release_sender, release_receiver) = mpsc::channel();
    let batcher = create_batcher(MockDependencies {
        view_state_reader_factory: Box::new(ParkedViewStateReaderFactory {
            entered_sender,
            release_receiver: Mutex::new(Some(release_receiver)),
        }),
        ..Default::default()
    })
    .await;
    let view_call_semaphore = batcher.view_call_semaphore.clone();

    let mut call_future = Box::pin(batcher.call_contract(undeployed_contract_call_input()));
    assert!(futures::poll!(&mut call_future).is_pending());
    entered_receiver.recv().await.expect("The view call should reach the state reader.");
    assert_eq!(view_call_semaphore.available_permits(), MAX_CONCURRENT_VIEW_CALLS - 1);

    // The caller gives up while the blocking thread is still parked.
    drop(call_future);
    assert_eq!(
        view_call_semaphore.available_permits(),
        MAX_CONCURRENT_VIEW_CALLS - 1,
        "The slot must stay taken while the blocking thread is parked."
    );

    release_sender.send(()).unwrap();
    // Acquiring every slot succeeds only once the parked thread returns the one it holds.
    let all_view_call_slots = tokio::time::timeout(
        Duration::from_secs(5),
        view_call_semaphore.acquire_many(u32::try_from(MAX_CONCURRENT_VIEW_CALLS).unwrap()),
    )
    .await
    .expect("The slot should be freed once the blocking task ends.");
    assert!(all_view_call_slots.is_ok());
}

#[tokio::test]
async fn call_contract_success() {
    let mut state = test_state(
        &ChainInfo::create_for_testing(),
        BALANCE,
        &[(STAKING_CONTRACT, 1), (ACCOUNT_CONTRACT, 1)],
    );

    let expected_retdata = vec![Felt::ONE, Felt::TWO, Felt::THREE];

    // Call the contract's setter entry point with the expected values.
    let account_address = ACCOUNT_CONTRACT.get_instance_address(0);
    let invoke_args = invoke_tx_args! {
        sender_address: account_address,
        calldata: create_calldata(STAKING_CONTRACT.get_instance_address(0), "set_current_epoch", &expected_retdata),
        resource_bounds: default_all_resource_bounds(),
        nonce: state.get_nonce_at(account_address).unwrap(),
    };
    let account_tx = invoke_tx_with_default_flags(invoke_args)
        .execute(&mut state, &BlockContext::create_for_testing())
        .unwrap();
    assert!(!account_tx.execute_call_info.unwrap().execution.failed);

    let batcher = create_batcher(MockDependencies {
        view_state_reader_factory: Box::new(TestViewStateReaderFactory {
            state: Arc::new(Mutex::new(state)),
            expected_block_number: INITIAL_HEIGHT,
        }),
        ..Default::default()
    })
    .await;

    let result = batcher
        .call_contract(CallContractInput {
            contract_address: STAKING_CONTRACT.get_instance_address(0),
            entry_point: "get_current_epoch_data".to_string(),
            calldata: vec![],
        })
        .await
        .unwrap();

    assert_eq!(result.retdata, vec![Felt::ONE, Felt::TWO, Felt::THREE]);
}

async fn create_batcher_with_recursive_contract(contract: FeatureContract) -> Batcher {
    let state = test_state(&ChainInfo::create_for_testing(), BALANCE, &[(contract, 1)]);
    create_batcher(MockDependencies {
        view_state_reader_factory: Box::new(TestViewStateReaderFactory {
            state: Arc::new(Mutex::new(state)),
            expected_block_number: INITIAL_HEIGHT,
        }),
        ..Default::default()
    })
    .await
}

fn recurse_call_contract_input(contract: FeatureContract, depth: u64) -> CallContractInput {
    CallContractInput {
        contract_address: contract.get_instance_address(0),
        entry_point: "recurse".to_string(),
        calldata: vec![Felt::from(depth)],
    }
}

#[rstest]
#[case::sierra_gas_tracked(SIERRA_GAS_TRACKED_RECURSIVE_CONTRACT)]
#[case::cairo_steps_tracked(CAIRO_STEPS_TRACKED_RECURSIVE_CONTRACT)]
#[tokio::test]
async fn call_contract_within_resource_bounds_succeeds(#[case] contract: FeatureContract) {
    let batcher = create_batcher_with_recursive_contract(contract).await;

    let result = batcher
        .call_contract(recurse_call_contract_input(
            contract,
            RECURSION_DEPTH_WITHIN_RESOURCE_BOUNDS,
        ))
        .await
        .unwrap();

    assert_eq!(result.retdata, vec![]);
}

#[rstest]
#[case::sierra_gas_bound(
    SIERRA_GAS_TRACKED_RECURSIVE_CONTRACT,
    SIERRA_GAS_RECURSION_DEPTH_EXCEEDING_RESOURCE_BOUNDS,
    "Out of gas"
)]
#[case::cairo_steps_bound(
    CAIRO_STEPS_TRACKED_RECURSIVE_CONTRACT,
    CAIRO_STEPS_RECURSION_DEPTH_EXCEEDING_RESOURCE_BOUNDS,
    "RunResources has no remaining steps."
)]
#[tokio::test]
async fn call_contract_exceeding_resource_bounds_fails(
    #[case] contract: FeatureContract,
    #[case] depth: u64,
    #[case] expected_reason: &str,
) {
    let batcher = create_batcher_with_recursive_contract(contract).await;

    let result = batcher.call_contract(recurse_call_contract_input(contract, depth)).await;

    assert_matches!(
        result,
        Err(BatcherError::ContractCallFailed { reason }) if reason.contains(expected_reason),
        "Expected the call to fail with {expected_reason:?}."
    );
}

#[rstest]
#[case::empty_retdata(0)]
#[case::retdata_below_the_limit(MAX_VIEW_CALL_RETDATA_LENGTH - 1)]
#[case::retdata_at_the_limit(MAX_VIEW_CALL_RETDATA_LENGTH)]
fn validate_retdata_length_accepts_lengths_up_to_the_limit(#[case] retdata_length: usize) {
    assert_eq!(validate_retdata_length(retdata_length), Ok(()));
}

/// The reason string is all the caller sees, so it must name both the returned length and the
/// limit.
#[rstest]
#[case::retdata_just_above_the_limit(MAX_VIEW_CALL_RETDATA_LENGTH + 1)]
#[case::retdata_far_above_the_limit(MAX_VIEW_CALL_RETDATA_LENGTH * 10)]
fn validate_retdata_length_rejects_length_above_the_limit(#[case] retdata_length: usize) {
    assert_matches!(
        validate_retdata_length(retdata_length),
        Err(BatcherError::ContractCallFailed { reason })
            if reason.contains(&retdata_length.to_string())
                && reason.contains(&MAX_VIEW_CALL_RETDATA_LENGTH.to_string())
    );
}

/// Writes `state_diff` to real batcher storage at `LAST_COMMITTED_HEIGHT` and reads `class_hash`
/// back through the view state reader the factory builds over it, at the height `call_contract`
/// would use.
async fn read_class_through_view_state_reader(
    state_diff: ThinStateDiff,
    class_manager_client: MockClassManagerClient,
    class_hash: ClassHash,
) -> StateResult<RunnableCompiledClass> {
    let ((storage_reader, mut storage_writer), _temp_dir) = get_test_storage();
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(LAST_COMMITTED_HEIGHT, state_diff)
        .unwrap()
        .commit()
        .unwrap();

    let factory = StorageViewStateReaderFactory {
        storage_reader,
        contract_class_manager: ContractClassManager::start(ContractClassManagerConfig::default()),
        class_manager_client: Arc::new(class_manager_client),
    };
    let state_reader = factory.create(
        LAST_COMMITTED_HEIGHT.unchecked_next(),
        NativeClassesWhitelist::All,
        tokio::runtime::Handle::current(),
        Duration::from_secs(300),
    );

    tokio::task::spawn_blocking(move || state_reader.get_compiled_class(class_hash))
        .await
        .expect("Reading a declared class panicked.")
}

/// Cairo 1 declaration marker only, with no definition behind it: `append_state_diff` records
/// `class_hash_to_compiled_class_hash` in the declared classes table that `is_declared` reads.
fn cairo_1_declaration(class_hash: ClassHash) -> ThinStateDiff {
    ThinStateDiff {
        class_hash_to_compiled_class_hash: indexmap! { class_hash => CompiledClassHash::default() },
        ..Default::default()
    }
}

/// Cairo 0 declaration marker only, with no definition behind it: `append_state_diff` records
/// `deprecated_declared_classes` in the deprecated classes table, which `is_declared` does not
/// read.
fn cairo_0_declaration(class_hash: ClassHash) -> ThinStateDiff {
    ThinStateDiff { deprecated_declared_classes: vec![class_hash], ..Default::default() }
}

/// The declaration marker `contract`'s class hash reaches the view state reader through.
fn declaration(contract: FeatureContract) -> ThinStateDiff {
    match contract.cairo_version() {
        CairoVersion::Cairo0 => cairo_0_declaration(contract.get_class_hash()),
        CairoVersion::Cairo1(_) => cairo_1_declaration(contract.get_class_hash()),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn view_state_reader_reads_cairo_1_class_through_the_class_manager() {
    let class_hash = class_hash!("0x1234");
    let mut class_manager_client = MockClassManagerClient::new();
    class_manager_client
        .expect_get_executable()
        .times(1)
        .with(eq(class_hash))
        .return_once(|_| Ok(Some(ContractClass::test_casm_contract_class())));
    class_manager_client
        .expect_get_sierra()
        .times(1)
        .with(eq(class_hash))
        .return_once(|_| Ok(Some(SierraContractClass::default())));

    let compiled_class = read_class_through_view_state_reader(
        cairo_1_declaration(class_hash),
        class_manager_client,
        class_hash,
    )
    .await;

    assert_matches!(compiled_class, Ok(RunnableCompiledClass::V1(_)));
}

/// Pins the route a declared Cairo 0 class takes. `is_declared` reads the Cairo 1 declared classes
/// table only, and `append_state_diff` writes `deprecated_declared_classes` to a different table,
/// so the class takes the deprecated route, which asks the class manager for the definition and
/// never reads storage. Were `is_declared` widened to the deprecated table, the class would take
/// the Cairo 1 route instead and panic in `ClassReader::read_casm`.
#[tokio::test(flavor = "multi_thread")]
async fn view_state_reader_reads_cairo_0_class_through_the_class_manager() {
    let class_hash = class_hash!("0x1234");
    let mut class_manager_client = MockClassManagerClient::new();
    class_manager_client
        .expect_get_executable()
        .times(1)
        .with(eq(class_hash))
        .return_once(|_| Ok(Some(ContractClass::test_deprecated_casm_contract_class())));

    let compiled_class = read_class_through_view_state_reader(
        cairo_0_declaration(class_hash),
        class_manager_client,
        class_hash,
    )
    .await;

    assert_matches!(compiled_class, Ok(RunnableCompiledClass::V0(_)));
}

#[tokio::test(flavor = "multi_thread")]
async fn view_state_reader_errors_when_the_class_manager_lacks_a_declared_class() {
    let class_hash = class_hash!("0x1234");
    let mut class_manager_client = MockClassManagerClient::new();
    class_manager_client
        .expect_get_executable()
        .times(1)
        .with(eq(class_hash))
        .return_once(|_| Ok(None));

    let result = read_class_through_view_state_reader(
        cairo_1_declaration(class_hash),
        class_manager_client,
        class_hash,
    )
    .await;

    assert_matches!(
        result,
        Err(StateError::UndeclaredClassHash(undeclared_class_hash))
            if undeclared_class_hash == class_hash
    );
}

/// A state reader whose every read parks until the sending half of `release_receiver` is dropped,
/// standing in for a class manager that never answers.
struct GatedStateReader {
    release_receiver: Arc<Mutex<Receiver<()>>>,
}

impl GatedStateReader {
    fn wait_for_release(&self) -> StateError {
        let _ = self.release_receiver.lock().unwrap().recv();
        StateError::StateReadError("Released.".to_string())
    }
}

impl StateReader for GatedStateReader {
    fn get_storage_at(&self, _address: ContractAddress, _key: StorageKey) -> StateResult<Felt> {
        Err(self.wait_for_release())
    }

    fn get_nonce_at(&self, _address: ContractAddress) -> StateResult<Nonce> {
        Err(self.wait_for_release())
    }

    fn get_class_hash_at(&self, _address: ContractAddress) -> StateResult<ClassHash> {
        Err(self.wait_for_release())
    }

    fn get_compiled_class(&self, _class_hash: ClassHash) -> StateResult<RunnableCompiledClass> {
        Err(self.wait_for_release())
    }

    fn get_compiled_class_hash(&self, _class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        Err(self.wait_for_release())
    }

    fn get_compiled_class_hash_v2(
        &self,
        _class_hash: ClassHash,
        _compiled_class: &RunnableCompiledClass,
    ) -> StateResult<CompiledClassHash> {
        Err(self.wait_for_release())
    }
}

struct GatedViewStateReaderFactory {
    release_receiver: Arc<Mutex<Receiver<()>>>,
}

impl ViewStateReaderFactory for GatedViewStateReaderFactory {
    fn create(
        &self,
        _block_number: BlockNumber,
        _native_classes_whitelist: NativeClassesWhitelist,
        _runtime: tokio::runtime::Handle,
        _class_manager_request_timeout: Duration,
    ) -> Box<dyn StateReader + Send> {
        Box::new(GatedStateReader { release_receiver: self.release_receiver.clone() })
    }
}

#[tokio::test(start_paused = true)]
async fn call_contract_times_out_when_the_state_reader_never_answers() {
    let (release_sender, release_receiver) = channel();

    let batcher = create_batcher(MockDependencies {
        view_state_reader_factory: Box::new(GatedViewStateReaderFactory {
            release_receiver: Arc::new(Mutex::new(release_receiver)),
        }),
        ..Default::default()
    })
    .await;
    let view_call_timeout = batcher.config.dynamic_config.view_call_timeout_millis;

    let mut call_contract_future = Box::pin(batcher.call_contract(CallContractInput {
        contract_address: Default::default(),
        entry_point: "get_stakers".to_string(),
        calldata: vec![],
    }));
    // The first poll registers the timeout's timer; advancing the paused clock then expires it.
    assert!(poll!(&mut call_contract_future).is_pending());
    tokio::time::advance(view_call_timeout).await;
    let Poll::Ready(result) = poll!(&mut call_contract_future) else {
        panic!("The call did not return once its timeout elapsed.");
    };

    assert_matches!(
        result,
        Err(BatcherError::ContractCallFailed { reason })
            if reason.contains(&view_call_timeout.as_secs().to_string())
    );

    // Let the blocked read finish, so the runtime can shut down.
    drop(release_sender);
}

/// Deploys `contract` in real batcher storage at `LAST_COMMITTED_HEIGHT`, and returns the
/// dependencies of a batcher whose view calls run over the real `StorageViewStateReaderFactory`.
/// The caller states the class manager expectations, so that each test owns the number of reads it
/// asserts on.
fn deploy_contract_in_real_storage(contract: FeatureContract) -> MockDependenciesWithRealStorage {
    deploy_contract_in_real_storage_with_commitment(
        contract,
        committed_block_hash_components(GasPrice(1)),
    )
}

/// The block-hash components a real commit writes, which is what a view call sources its block info
/// from. Every price is set to `gas_price`, so a test can state one and assert it survives.
fn committed_block_hash_components(gas_price: GasPrice) -> PartialBlockHashComponents {
    let price_per_token = GasPricePerToken { price_in_wei: gas_price, price_in_fri: gas_price };
    PartialBlockHashComponents {
        block_number: LAST_COMMITTED_HEIGHT,
        l1_gas_price: price_per_token,
        l1_data_gas_price: price_per_token,
        l2_gas_price: price_per_token,
        timestamp: COMMITTED_BLOCK_TIMESTAMP,
        ..Default::default()
    }
}

/// Writes no block header: nothing in production writes one to the batcher's storage, so seeding
/// one would assert against a precondition the node never has.
fn deploy_contract_in_real_storage_with_commitment(
    contract: FeatureContract,
    block_hash_components: PartialBlockHashComponents,
) -> MockDependenciesWithRealStorage {
    let class_hash = contract.get_class_hash();
    let mut mock_dependencies = MockDependenciesWithRealStorage::default();

    mock_dependencies
        .storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(
            LAST_COMMITTED_HEIGHT,
            ThinStateDiff {
                deployed_contracts: indexmap! { contract.get_instance_address(0) => class_hash },
                ..declaration(contract)
            },
        )
        .unwrap()
        // The commitment manager panics on a committed height with no hash commitment behind it.
        .set_partial_block_hash_components(&LAST_COMMITTED_HEIGHT, &block_hash_components)
        .unwrap()
        .commit()
        .unwrap();

    mock_dependencies
}

/// Drives `call_contract` over the real `StorageViewStateReaderFactory`, the composition the node
/// runs: the runtime handle reaches the factory, the reader lands inside `spawn_blocking`, and the
/// class read blocks on the class manager from that thread. Running the call on the async runtime
/// instead panics in `ClassReader::block_on`.
#[rstest]
#[case::cairo_1(SIERRA_GAS_TRACKED_RECURSIVE_CONTRACT)]
#[case::cairo_0(CAIRO_STEPS_TRACKED_RECURSIVE_CONTRACT)]
#[tokio::test(flavor = "multi_thread")]
async fn call_contract_over_real_storage_executes_a_class_from_the_class_manager(
    #[case] contract: FeatureContract,
) {
    let class_hash = contract.get_class_hash();
    let mut mock_dependencies = deploy_contract_in_real_storage(contract);
    mock_dependencies
        .class_manager_client
        .expect_get_executable()
        .times(1)
        .with(eq(class_hash))
        .returning(move |_| Ok(Some(contract.get_class())));
    // The Cairo 1 route reads the Sierra too, for its version. The Cairo 0 route reads the
    // executable alone.
    if matches!(contract.cairo_version(), CairoVersion::Cairo1(_)) {
        mock_dependencies
            .class_manager_client
            .expect_get_sierra()
            .times(1)
            .with(eq(class_hash))
            .returning(move |_| Ok(Some(contract.get_sierra())));
    }
    let batcher = create_batcher_with_real_storage(mock_dependencies).await;

    let result = batcher
        .call_contract(recurse_call_contract_input(
            contract,
            RECURSION_DEPTH_WITHIN_RESOURCE_BOUNDS,
        ))
        .await;

    assert_eq!(result.unwrap().retdata, vec![]);
}

/// Two view calls fetch the class once: the second is served from the batcher's class cache, which
/// the view path and block production build over one `ContractClassManager`.
#[tokio::test(flavor = "multi_thread")]
async fn repeated_view_calls_fetch_the_class_once() {
    let contract = SIERRA_GAS_TRACKED_RECURSIVE_CONTRACT;
    let class_hash = contract.get_class_hash();
    let mut mock_dependencies = deploy_contract_in_real_storage(contract);
    mock_dependencies
        .class_manager_client
        .expect_get_executable()
        .times(1)
        .with(eq(class_hash))
        .returning(move |_| Ok(Some(contract.get_class())));
    mock_dependencies
        .class_manager_client
        .expect_get_sierra()
        .times(1)
        .with(eq(class_hash))
        .returning(move |_| Ok(Some(contract.get_sierra())));
    let batcher = create_batcher_with_real_storage(mock_dependencies).await;

    for _ in 0..2 {
        batcher
            .call_contract(recurse_call_contract_input(
                contract,
                RECURSION_DEPTH_WITHIN_RESOURCE_BOUNDS,
            ))
            .await
            .unwrap();
    }
}

/// A committed block may carry a zero gas price, and a view call spends no fee, so it must not be
/// the reason the call fails.
#[tokio::test(flavor = "multi_thread")]
async fn call_contract_over_a_committed_zero_gas_price_succeeds() {
    let contract = SIERRA_GAS_TRACKED_RECURSIVE_CONTRACT;
    let mut mock_dependencies = deploy_contract_in_real_storage_with_commitment(
        contract,
        committed_block_hash_components(GasPrice(0)),
    );
    mock_dependencies
        .class_manager_client
        .expect_get_executable()
        .returning(move |_| Ok(Some(contract.get_class())));
    mock_dependencies
        .class_manager_client
        .expect_get_sierra()
        .returning(move |_| Ok(Some(contract.get_sierra())));
    let batcher = create_batcher_with_real_storage(mock_dependencies).await;

    let result = batcher
        .call_contract(recurse_call_contract_input(
            contract,
            RECURSION_DEPTH_WITHIN_RESOURCE_BOUNDS,
        ))
        .await;

    assert_eq!(result.unwrap().retdata, vec![]);
}

/// A block committed before block hash components existed has no block info to build a context
/// from. The caller is told that, rather than being handed a bare internal error.
#[tokio::test]
async fn call_contract_without_stored_block_info_names_what_is_missing() {
    let mut storage_reader = mock_storage_reader();
    storage_reader.expect_state_diff_height().returning(|| Ok(INITIAL_HEIGHT));
    storage_reader.expect_global_root_height().returning(|| Ok(INITIAL_HEIGHT));
    storage_reader.expect_get_partial_block_hash_components().returning(|_| Ok(None));
    let batcher = create_batcher(MockDependencies { storage_reader, ..Default::default() }).await;

    let result = batcher
        .call_contract(CallContractInput {
            contract_address: Default::default(),
            entry_point: "get_stakers".to_string(),
            calldata: vec![],
        })
        .await;

    assert_matches!(
        result,
        Err(BatcherError::ContractCallFailed { reason }) if reason.contains("No block info stored")
    );
}

/// The data availability mode is committed to by the block hash, so the block info reported for a
/// committed block must be the mode that block was built with.
#[rstest]
#[case::calldata(L1DataAvailabilityMode::Calldata)]
#[case::blob(L1DataAvailabilityMode::Blob)]
#[tokio::test]
async fn block_info_reports_the_committed_data_availability_mode(
    #[case] l1_da_mode: L1DataAvailabilityMode,
) {
    let mut storage_reader = mock_storage_reader();
    storage_reader.expect_state_diff_height().returning(|| Ok(INITIAL_HEIGHT));
    storage_reader.expect_global_root_height().returning(|| Ok(INITIAL_HEIGHT));
    storage_reader.expect_get_partial_block_hash_components().returning(move |_| {
        Ok(Some(PartialBlockHashComponents {
            header_commitments: BlockHeaderCommitments {
                concatenated_counts: concat_counts(0, 0, 0, l1_da_mode),
                ..Default::default()
            },
            ..Default::default()
        }))
    });
    let batcher = create_batcher(MockDependencies { storage_reader, ..Default::default() }).await;

    let block_info = batcher.get_block_info(LAST_COMMITTED_HEIGHT).unwrap();

    assert_eq!(block_info.use_kzg_da, l1_da_mode.is_use_kzg_da());
}

const STATE_COMMITMENT_INFOS_RETENTION_BLOCKS: u64 = 10;
const MAX_STATE_COMMITMENT_INFOS_DELETIONS_PER_PRUNE: usize = 10;
const PRUNED_STATE_COMMITMENT_INFOS_DATA_END_OFFSET: usize = 4096;
/// The storage height the batcher reports in the state commitment infos pruning tests.
const STATE_COMMITMENT_INFOS_PRUNING_STORAGE_HEIGHT: BlockNumber = BlockNumber(30);

fn mock_dependencies_for_state_commitment_infos_pruning() -> MockDependencies {
    let mut storage_reader = mock_storage_reader();
    storage_reader
        .expect_state_diff_height()
        .returning(|| Ok(STATE_COMMITMENT_INFOS_PRUNING_STORAGE_HEIGHT));
    storage_reader
        .expect_global_root_height()
        .returning(|| Ok(STATE_COMMITMENT_INFOS_PRUNING_STORAGE_HEIGHT));
    let mut mock_dependencies = MockDependencies { storage_reader, ..Default::default() };
    mock_dependencies.batcher_config.static_config.state_commitment_infos_pruning_config =
        StateCommitmentInfosPruningConfig {
            retention_blocks: STATE_COMMITMENT_INFOS_RETENTION_BLOCKS,
            max_deletions_per_prune: MAX_STATE_COMMITMENT_INFOS_DELETIONS_PER_PRUNE,
        };
    mock_dependencies
}

fn retention_window_start(height: BlockNumber) -> BlockNumber {
    BlockNumber(height.0 - STATE_COMMITMENT_INFOS_RETENTION_BLOCKS)
}

/// Expects a prune up to `prune_below` that advances the lower bound to `new_lower_bound`,
/// followed by releasing the space of the pruned infos.
fn expect_prune_state_commitment_infos(
    storage_writer: &mut MockBatcherStorageWriter,
    prune_below: BlockNumber,
    new_lower_bound: BlockNumber,
) {
    storage_writer
        .expect_prune_state_commitment_infos_pointers()
        .times(1)
        .with(eq(prune_below), eq(MAX_STATE_COMMITMENT_INFOS_DELETIONS_PER_PRUNE))
        .returning(move |_, _| {
            Ok(Some(PrunedStateCommitmentInfosPointers {
                new_lower_bound,
                data_end_offset: PRUNED_STATE_COMMITMENT_INFOS_DATA_END_OFFSET,
            }))
        });
    storage_writer
        .expect_prune_state_commitment_infos_data()
        .times(1)
        .with(eq(PRUNED_STATE_COMMITMENT_INFOS_DATA_END_OFFSET))
        .returning(|_| Ok(()));
}

fn prune_state_commitment_infos(batcher: &mut Batcher, height: BlockNumber) {
    batcher.prune_state_commitment_infos(PruneStateCommitmentInfosInput { height }).unwrap();
}

#[tokio::test]
async fn prune_state_commitment_infos_below_retention_window_and_release_space() {
    let mut mock_dependencies = mock_dependencies_for_state_commitment_infos_pruning();
    let height = BlockNumber(20);
    let prune_below = retention_window_start(height);
    expect_prune_state_commitment_infos(
        &mut mock_dependencies.storage_writer,
        prune_below,
        prune_below,
    );

    let mut batcher = create_batcher(mock_dependencies).await;
    prune_state_commitment_infos(&mut batcher, height);
}

/// The requested height may be ahead of the heights this batcher stored, so the retention window
/// is measured from its storage height instead.
#[tokio::test]
async fn prune_state_commitment_infos_below_the_storage_height_retention_window() {
    let mut mock_dependencies = mock_dependencies_for_state_commitment_infos_pruning();
    let prune_below = retention_window_start(STATE_COMMITMENT_INFOS_PRUNING_STORAGE_HEIGHT);
    expect_prune_state_commitment_infos(
        &mut mock_dependencies.storage_writer,
        prune_below,
        prune_below,
    );

    let mut batcher = create_batcher(mock_dependencies).await;
    prune_state_commitment_infos(
        &mut batcher,
        BlockNumber(STATE_COMMITMENT_INFOS_PRUNING_STORAGE_HEIGHT.0 + 100),
    );
}

#[tokio::test]
async fn prune_state_commitment_infos_releases_no_space_when_nothing_is_pruned() {
    let mut mock_dependencies = mock_dependencies_for_state_commitment_infos_pruning();
    mock_dependencies
        .storage_writer
        .expect_prune_state_commitment_infos_pointers()
        .times(1)
        .returning(|_, _| Ok(None));
    // No expectation is set for punching a hole: the mock panics if it is called.

    let mut batcher = create_batcher(mock_dependencies).await;
    prune_state_commitment_infos(&mut batcher, BlockNumber(20));
}

#[tokio::test]
async fn prune_state_commitment_infos_storage_failure_is_internal_error() {
    let mut mock_dependencies = mock_dependencies_for_state_commitment_infos_pruning();
    mock_dependencies
        .storage_writer
        .expect_prune_state_commitment_infos_pointers()
        .times(1)
        .returning(|_, _| {
            Err(apollo_storage::StorageError::DBInconsistency { msg: String::new() })
        });

    let mut batcher = create_batcher(mock_dependencies).await;
    let result = batcher
        .prune_state_commitment_infos(PruneStateCommitmentInfosInput { height: BlockNumber(20) });
    assert_eq!(result, Err(BatcherError::InternalError));
}

/// The pointers are deleted in their own transaction and the data they pointed at is released
/// only afterwards, so a failure in between leaves the data of already-deleted pointers behind.
/// The next prune releases the data of its own pointers and of those left behind, because
/// releasing the data below an offset releases everything below it.
#[tokio::test]
async fn prune_state_commitment_infos_releases_the_data_of_previously_pruned_pointers() {
    const FIRST_DATA_END_OFFSET: usize = 4096;
    const SECOND_DATA_END_OFFSET: usize = 8192;
    let height = BlockNumber(20);
    let prune_below = retention_window_start(height);

    let mut mock_dependencies = mock_dependencies_for_state_commitment_infos_pruning();
    let storage_writer = &mut mock_dependencies.storage_writer;
    // The first prune deletes the pointers of the lowest stored heights...
    storage_writer
        .expect_prune_state_commitment_infos_pointers()
        .times(1)
        .with(eq(prune_below), eq(MAX_STATE_COMMITMENT_INFOS_DELETIONS_PER_PRUNE))
        .returning(|_, _| {
            Ok(Some(PrunedStateCommitmentInfosPointers {
                new_lower_bound: BlockNumber(5),
                data_end_offset: FIRST_DATA_END_OFFSET,
            }))
        });
    // ... but releasing their data fails, so it is left with no pointer into it.
    storage_writer
        .expect_prune_state_commitment_infos_data()
        .times(1)
        .with(eq(FIRST_DATA_END_OFFSET))
        .returning(|_| Err(StorageError::DBInconsistency { msg: String::new() }));
    // The next prune deletes the pointers of the following heights...
    storage_writer
        .expect_prune_state_commitment_infos_pointers()
        .times(1)
        .with(eq(prune_below), eq(MAX_STATE_COMMITMENT_INFOS_DELETIONS_PER_PRUNE))
        .returning(move |_, _| {
            Ok(Some(PrunedStateCommitmentInfosPointers {
                new_lower_bound: prune_below,
                data_end_offset: SECOND_DATA_END_OFFSET,
            }))
        });
    // ... and releases their data.
    let released_up_to = Arc::new(Mutex::new(None));
    let recorded_released_up_to = released_up_to.clone();
    storage_writer.expect_prune_state_commitment_infos_data().times(1).returning(move |end| {
        *recorded_released_up_to.lock().unwrap() = Some(end);
        Ok(())
    });

    let mut batcher = create_batcher(mock_dependencies).await;
    assert_eq!(
        batcher.prune_state_commitment_infos(PruneStateCommitmentInfosInput { height }),
        Err(BatcherError::InternalError)
    );
    prune_state_commitment_infos(&mut batcher, height);

    let released_up_to =
        released_up_to.lock().unwrap().expect("The pruned data should have been released.");
    for pruned_data_end_offset in [FIRST_DATA_END_OFFSET, SECOND_DATA_END_OFFSET] {
        assert!(
            released_up_to >= pruned_data_end_offset,
            "The data of the pointers pruned up to offset {pruned_data_end_offset} was not \
             released; only the data below {released_up_to} was."
        );
    }
}
