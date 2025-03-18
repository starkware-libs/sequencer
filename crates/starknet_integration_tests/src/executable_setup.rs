use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use blockifier::context::ChainInfo;
use mempool_test_utils::starknet_api_test_utils::AccountTransactionGenerator;
use papyrus_base_layer::ethereum_base_layer_contract::EthereumBaseLayerConfig;
use papyrus_storage::StorageConfig;
use starknet_api::execution_resources::GasAmount;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_class_manager::test_utils::FileHandles;
use starknet_consensus_manager::config::ConsensusManagerConfig;
use starknet_http_server::config::HttpServerConfig;
use starknet_http_server::test_utils::HttpTestClient;
use starknet_infra_utils::test_utils::AvailablePorts;
use starknet_mempool_p2p::config::MempoolP2pConfig;
use starknet_monitoring_endpoint::config::MonitoringEndpointConfig;
use starknet_monitoring_endpoint::test_utils::MonitoringClient;
use starknet_sequencer_node::config::component_config::ComponentConfig;
use starknet_sequencer_node::config::config_utils::dump_config_file;
use starknet_sequencer_node::config::definitions::ConfigPointersMap;
use starknet_sequencer_node::config::node_config::{
    SequencerNodeConfig,
    CONFIG_NON_POINTERS_WHITELIST,
};
use starknet_sequencer_node::test_utils::node_runner::NodeRunner;
use starknet_state_sync::config::StateSyncConfig;
use tempfile::{tempdir, TempDir};
use tokio::fs::create_dir_all;
use tracing::instrument;

use crate::state_reader::{
    StorageTestSetup,
    BATCHER_DB_PATH_SUFFIX,
    CLASSES_STORAGE_DB_PATH_SUFFIX,
    CLASS_HASH_STORAGE_DB_PATH_SUFFIX,
    CLASS_MANAGER_DB_PATH_SUFFIX,
    STATE_SYNC_DB_PATH_SUFFIX,
};
use crate::utils::{create_node_config, spawn_local_success_recorder};

// TODO(Tsabary): rename this module to `executable_setup`.

const NODE_CONFIG_CHANGES_FILE_PATH: &str = "node_integration_test_config_changes.json";

#[derive(Debug, Copy, Clone)]
pub struct NodeExecutionId {
    node_index: usize,
    executable_index: usize,
}

impl NodeExecutionId {
    pub fn new(node_index: usize, executable_index: usize) -> Self {
        Self { node_index, executable_index }
    }
    pub fn get_node_index(&self) -> usize {
        self.node_index
    }
    pub fn get_executable_index(&self) -> usize {
        self.executable_index
    }

    pub fn build_path(&self, base: &Path) -> PathBuf {
        base.join(format!("node_{}", self.node_index))
            .join(format!("executable_{}", self.executable_index))
    }
}

impl std::fmt::Display for NodeExecutionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Node id {} part {}", self.node_index, self.executable_index)
    }
}

impl From<NodeExecutionId> for NodeRunner {
    fn from(val: NodeExecutionId) -> Self {
        NodeRunner::new(val.node_index, val.executable_index)
    }
}

pub struct ExecutableSetup {
    // Node test identifier.
    pub node_execution_id: NodeExecutionId,
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
    // Config values.
    pub config: SequencerNodeConfig,
    // Configuration parameters that share the same value across multiple components.
    pub config_pointers_map: ConfigPointersMap,
    // Handlers for the storage and config files, maintained so the files are not deleted. Since
    // these are only maintained to avoid dropping the handlers, private visibility suffices, and
    // as such, the '#[allow(dead_code)]' attributes are used to suppress the warning.
    #[allow(dead_code)]
    batcher_storage_handle: Option<TempDir>,
    #[allow(dead_code)]
    node_config_dir_handle: Option<TempDir>,
    #[allow(dead_code)]
    state_sync_storage_handle: Option<TempDir>,
    #[allow(dead_code)]
    class_manager_storage_handles: Option<FileHandles>,
}

// TODO(Tsabary/ Nadin): reduce number of args.
#[allow(clippy::too_many_arguments)]
impl ExecutableSetup {
    #[instrument(skip(accounts, chain_info, consensus_manager_config), level = "debug")]
    pub async fn new(
        accounts: Vec<AccountTransactionGenerator>,
        node_execution_id: NodeExecutionId,
        chain_info: ChainInfo,
        mut consensus_manager_config: ConsensusManagerConfig,
        mempool_p2p_config: MempoolP2pConfig,
        state_sync_config: StateSyncConfig,
        mut available_ports: AvailablePorts,
        component_config: ComponentConfig,
        base_layer_config: EthereumBaseLayerConfig,
        db_path_dir: Option<PathBuf>,
        config_path_dir: Option<PathBuf>,
        exec_data_prefix_dir: Option<PathBuf>,
    ) -> Self {
        // TODO(Nadin): pass the test storage as an argument.
        // Creating the storage for the test.
        let StorageTestSetup {
            mut batcher_storage_config,
            batcher_storage_handle,
            mut state_sync_storage_config,
            state_sync_storage_handle,
            mut class_manager_storage_config,
            class_manager_storage_handles,
        } = StorageTestSetup::new(accounts, &chain_info, db_path_dir);

        // Allow overriding the path with a custom prefix for Docker mode in system tests.
        if let Some(ref prefix) = exec_data_prefix_dir {
            batcher_storage_config.db_config.path_prefix = prefix.join(BATCHER_DB_PATH_SUFFIX);
            state_sync_storage_config.db_config.path_prefix =
                prefix.join(STATE_SYNC_DB_PATH_SUFFIX);
            class_manager_storage_config.class_hash_storage_config.path_prefix =
                prefix.join(CLASS_MANAGER_DB_PATH_SUFFIX).join(CLASS_HASH_STORAGE_DB_PATH_SUFFIX);
            class_manager_storage_config.persistent_root =
                prefix.join(CLASS_MANAGER_DB_PATH_SUFFIX).join(CLASSES_STORAGE_DB_PATH_SUFFIX);
        }

        let (recorder_url, _join_handle) =
            spawn_local_success_recorder(available_ports.get_next_port());
        consensus_manager_config.cende_config.recorder_url = recorder_url;

        // Explicitly collect metrics in the monitoring endpoint.
        let monitoring_endpoint_config = MonitoringEndpointConfig {
            port: available_ports.get_next_port(),
            collect_metrics: true,
            ..Default::default()
        };

        let block_max_capacity_n_steps = GasAmount(30000000);
        // Derive the configuration for the sequencer node.
        let (config, config_pointers_map) = create_node_config(
            &mut available_ports,
            node_execution_id,
            chain_info,
            batcher_storage_config,
            state_sync_storage_config,
            class_manager_storage_config,
            state_sync_config,
            consensus_manager_config,
            mempool_p2p_config,
            monitoring_endpoint_config,
            component_config,
            base_layer_config,
            block_max_capacity_n_steps,
        );

        let (node_config_dir, node_config_dir_handle) = match config_path_dir {
            Some(config_path_dir) => {
                create_dir_all(&config_path_dir).await.unwrap();
                (config_path_dir, None)
            }
            None => {
                let node_config_dir_handle = tempdir().unwrap();
                (node_config_dir_handle.path().to_path_buf(), Some(node_config_dir_handle))
            }
        };
        let node_config_path = node_config_dir.join(NODE_CONFIG_CHANGES_FILE_PATH);
        // Wait for the node to start.
        let MonitoringEndpointConfig { ip, port, .. } = config.monitoring_endpoint_config;
        let monitoring_client = MonitoringClient::new(SocketAddr::from((ip, port)));

        let HttpServerConfig { ip, port } = config.http_server_config;
        let add_tx_http_client = HttpTestClient::new(SocketAddr::from((ip, port)));

        let executable_setup = Self {
            node_execution_id,
            add_tx_http_client,
            monitoring_client,
            batcher_storage_handle,
            batcher_storage_config: config.batcher_config.storage.clone(),
            config: config.clone(),
            config_pointers_map,
            node_config_dir_handle,
            node_config_path,
            state_sync_storage_handle,
            state_sync_storage_config: config.state_sync_config.storage_config,
            class_manager_storage_handles,
        };
        executable_setup.dump_config_file_changes();
        executable_setup
    }

    pub async fn assert_add_tx_success(&self, tx: RpcTransaction) -> TransactionHash {
        self.add_tx_http_client.assert_add_tx_success(tx).await
    }

    pub fn modify_config<F>(&mut self, modify_config_fn: F)
    where
        F: Fn(&mut SequencerNodeConfig),
    {
        modify_config_fn(&mut self.config);
        self.dump_config_file_changes();
    }

    pub fn modify_config_pointers<F>(&mut self, modify_config_pointers_fn: F)
    where
        F: Fn(&mut ConfigPointersMap),
    {
        modify_config_pointers_fn(&mut self.config_pointers_map);
        self.dump_config_file_changes();
    }

    /// Creates a config file for the sequencer node for an integration test.
    pub fn dump_config_file_changes(&self) {
        dump_config_file(
            self.config.clone(),
            &self.config_pointers_map.clone().into(),
            &CONFIG_NON_POINTERS_WHITELIST,
            &self.node_config_path,
        );
    }
}
