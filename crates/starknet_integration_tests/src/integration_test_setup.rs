use std::net::SocketAddr;
use std::path::PathBuf;

use blockifier::context::ChainInfo;
use mempool_test_utils::starknet_api_test_utils::AccountTransactionGenerator;
use papyrus_storage::StorageConfig;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_consensus_manager::config::ConsensusManagerConfig;
use starknet_http_server::config::HttpServerConfig;
use starknet_http_server::test_utils::HttpTestClient;
use starknet_infra_utils::test_utils::AvailablePorts;
use starknet_mempool_p2p::config::MempoolP2pConfig;
use starknet_monitoring_endpoint::config::MonitoringEndpointConfig;
use starknet_monitoring_endpoint::test_utils::MonitoringClient;
use starknet_sequencer_node::config::component_config::ComponentConfig;
use starknet_sequencer_node::test_utils::node_runner::NodeRunner;
use starknet_state_sync::config::StateSyncConfig;
use tempfile::{tempdir, TempDir};
use tracing::instrument;

use crate::config_utils::dump_config_file_changes;
use crate::state_reader::StorageTestSetup;
use crate::utils::{create_node_config, spawn_local_success_recorder};

// TODO(Nadin): rename to NodeExecutionId.
#[derive(Debug, Copy, Clone)]
pub struct SequencerExecutionId {
    sequencer_index: usize,
    sequencer_part_index: usize,
}

impl SequencerExecutionId {
    pub fn new(sequencer_index: usize, sequencer_part_index: usize) -> Self {
        Self { sequencer_index, sequencer_part_index }
    }
    pub fn get_sequencer_index(&self) -> usize {
        self.sequencer_index
    }
    pub fn get_sequencer_part_index(&self) -> usize {
        self.sequencer_part_index
    }
}

impl From<SequencerExecutionId> for NodeRunner {
    fn from(val: SequencerExecutionId) -> Self {
        NodeRunner::new(val.sequencer_index, val.sequencer_part_index)
    }
}

pub struct ExecutableSetup {
    // Sequencer test identifier.
    pub sequencer_execution_id: SequencerExecutionId,
    // Client for adding transactions to the sequencer node.
    pub add_tx_http_client: HttpTestClient,
    // Client for checking liveness of the sequencer node.
    pub monitoring_client: MonitoringClient,
    // Path to the node configuration file.
    pub node_config_path: PathBuf,
    // Storage reader for the batcher.
    pub batcher_storage_config: StorageConfig,
    // Storage reader for the state sync.
    pub state_sync_storage_config: StorageConfig,
    // Handlers for the storage and config files, maintained so the files are not deleted. Since
    // these are only maintained to avoid dropping the handlers, private visibility suffices, and
    // as such, the '#[allow(dead_code)]' attributes are used to suppress the warning.
    #[allow(dead_code)]
    batcher_storage_handle: TempDir,
    #[allow(dead_code)]
    node_config_dir_handle: TempDir,
    #[allow(dead_code)]
    state_sync_storage_handle: TempDir,
}

// TODO(Tsabary/ Nadin): reduce number of args.
#[allow(clippy::too_many_arguments)]
impl ExecutableSetup {
    #[instrument(skip(accounts, chain_info, consensus_manager_config), level = "debug")]
    pub async fn new(
        accounts: Vec<AccountTransactionGenerator>,
        sequencer_execution_id: SequencerExecutionId,
        chain_info: ChainInfo,
        mut consensus_manager_config: ConsensusManagerConfig,
        mempool_p2p_config: MempoolP2pConfig,
        mut state_sync_config: StateSyncConfig,
        mut available_ports: AvailablePorts,
        component_config: ComponentConfig,
    ) -> Self {
        // TODO(Nadin): pass the test storage as an argument.
        // Creating the storage for the test.
        let storage_for_test = StorageTestSetup::new(accounts, &chain_info);

        let (recorder_url, _join_handle) =
            spawn_local_success_recorder(available_ports.get_next_port());
        consensus_manager_config.cende_config.recorder_url = recorder_url;

        state_sync_config.storage_config = storage_for_test.state_sync_storage_config;

        // Derive the configuration for the sequencer node.
        let (config, required_params) = create_node_config(
            &mut available_ports,
            sequencer_execution_id,
            chain_info,
            storage_for_test.batcher_storage_config,
            state_sync_config,
            consensus_manager_config,
            mempool_p2p_config,
            component_config,
        )
        .await;

        let node_config_dir_handle = tempdir().unwrap();
        let node_config_path = dump_config_file_changes(
            &config,
            required_params,
            node_config_dir_handle.path().to_path_buf(),
        );

        // Wait for the node to start.
        let MonitoringEndpointConfig { ip, port, .. } = config.monitoring_endpoint_config;
        let monitoring_client = MonitoringClient::new(SocketAddr::from((ip, port)));

        let HttpServerConfig { ip, port } = config.http_server_config;
        let add_tx_http_client = HttpTestClient::new(SocketAddr::from((ip, port)));

        Self {
            sequencer_execution_id,
            add_tx_http_client,
            monitoring_client,
            batcher_storage_handle: storage_for_test.batcher_storage_handle,
            batcher_storage_config: config.batcher_config.storage,
            node_config_dir_handle,
            node_config_path,
            state_sync_storage_handle: storage_for_test.state_sync_storage_handle,
            state_sync_storage_config: config.state_sync_config.storage_config,
        }
    }

    pub async fn assert_add_tx_success(&self, tx: RpcTransaction) -> TransactionHash {
        self.add_tx_http_client.assert_add_tx_success(tx).await
    }
}
