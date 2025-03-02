use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use blockifier::context::ChainInfo;
use mempool_test_utils::starknet_api_test_utils::AccountTransactionGenerator;
use papyrus_base_layer::ethereum_base_layer_contract::EthereumBaseLayerConfig;
use papyrus_config::dumping::{
    combine_config_map_and_pointers,
    ConfigPointers,
    Pointers,
    SerializeConfig,
};
use papyrus_config::{ParamPath, SerializedContent, SerializedParam};
use papyrus_storage::StorageConfig;
use serde_json::Value;
use starknet_api::block::BlockNumber;
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
use starknet_sequencer_node::config::config_utils::{
    config_to_preset,
    dump_json_data,
    RequiredParams,
};
use starknet_sequencer_node::config::node_config::{
    SequencerNodeConfig,
    CONFIG_NON_POINTERS_WHITELIST,
    CONFIG_POINTERS,
};
use starknet_sequencer_node::test_utils::node_runner::NodeRunner;
use starknet_state_sync::config::StateSyncConfig;
use tempfile::{tempdir, TempDir};
use tokio::fs::create_dir_all;
use tracing::instrument;
use url::Url;

use crate::state_reader::StorageTestSetup;
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

#[derive(Clone)]
struct ConfigPointersMap(HashMap<ParamPath, (SerializedParam, Pointers)>);

impl ConfigPointersMap {
    fn new(config_pointers: ConfigPointers) -> Self {
        ConfigPointersMap(config_pointers.into_iter().map(|((k, v), p)| (k, (v, p))).collect())
    }

    fn change_target_value(&mut self, target: &str, value: Value) {
        assert!(self.0.contains_key(target));
        self.0.entry(target.to_owned()).and_modify(|(param, _)| {
            param.content = SerializedContent::DefaultValue(value);
        });
    }
}

impl From<ConfigPointersMap> for ConfigPointers {
    fn from(config_pointers_map: ConfigPointersMap) -> Self {
        config_pointers_map.0.into_iter().map(|(k, (v, p))| ((k, v), p)).collect()
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
    // Required param values.
    required_params: RequiredParams,
    // Configuration parameters that share the same value across multiple components.
    config_pointers_map: ConfigPointersMap,
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
        db_path_dir: Option<PathBuf>,
        config_path_dir: Option<PathBuf>,
    ) -> Self {
        // TODO(Nadin): pass the test storage as an argument.
        // Creating the storage for the test.
        let StorageTestSetup {
            batcher_storage_config,
            batcher_storage_handle,
            state_sync_storage_config,
            state_sync_storage_handle,
            class_manager_storage_config,
            class_manager_storage_handles,
        } = StorageTestSetup::new(accounts, &chain_info, db_path_dir);

        let (recorder_url, _join_handle) =
            spawn_local_success_recorder(available_ports.get_next_port());
        consensus_manager_config.cende_config.recorder_url = recorder_url;

        // Explicitly collect metrics in the monitoring endpoint.
        let monitoring_endpoint_config = MonitoringEndpointConfig {
            port: available_ports.get_next_port(),
            collect_metrics: true,
            ..Default::default()
        };

        let base_layer_config = EthereumBaseLayerConfig {
            node_url: Url::parse("https://node_url").expect("Should be a valid URL"),
            ..Default::default()
        };
        // Derive the configuration for the sequencer node.
        let (config, required_params) = create_node_config(
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
            required_params,
            config_pointers_map: ConfigPointersMap::new(CONFIG_POINTERS.clone()),
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

    // TODO(noamsp): Change this into change_config once we need to change other values in the
    // config.
    pub fn update_revert_config(&mut self, value: Option<BlockNumber>) {
        match value {
            Some(value) => {
                self.config_pointers_map.change_target_value(
                    "revert_config.revert_up_to_and_including",
                    Value::from(value.0),
                );
                self.config.state_sync_config.revert_config.revert_up_to_and_including = value;
                self.config.consensus_manager_config.revert_config.revert_up_to_and_including =
                    value;

                self.config_pointers_map
                    .change_target_value("revert_config.should_revert", Value::from(true));
                self.config.state_sync_config.revert_config.should_revert = true;
                self.config.consensus_manager_config.revert_config.should_revert = true;
            }
            // If should revert is false, the revert_up_to_and_including value is irrelevant.
            None => {
                self.config_pointers_map
                    .change_target_value("revert_config.should_revert", Value::from(false));
                self.config.state_sync_config.revert_config.should_revert = false;
                self.config.consensus_manager_config.revert_config.should_revert = false;
            }
        }
        self.dump_config_file_changes();
    }

    /// Creates a config file for the sequencer node for an integration test.
    pub fn dump_config_file_changes(&self) {
        // Create the entire mapping of the config and the pointers, without the required params.
        let config_as_map = combine_config_map_and_pointers(
            self.config.dump(),
            &self.config_pointers_map.clone().into(),
            &CONFIG_NON_POINTERS_WHITELIST,
        )
        .unwrap();

        // Extract only the required fields from the config map.
        let mut preset = config_to_preset(&config_as_map);

        // Add the required params to the preset.
        add_required_params_to_preset(&mut preset, self.required_params.as_json());

        // Dump the preset to a file, return its path.
        dump_json_data(preset, &self.node_config_path);
        assert!(
            &self.node_config_path.exists(),
            "File does not exist: {:?}",
            &self.node_config_path
        );
    }
}

/// Merges required parameters into an existing preset JSON object.
///
/// # Parameters
/// - `preset`: A mutable reference to a `serde_json::Value` representing the preset. It must be a
///   JSON dictionary object where additional parameters will be added.
/// - `required_params`: A reference to a `serde_json::Value` representing the required parameters.
///   It must also be a JSON dictionary object. Its keys and values will be merged into the
///   `preset`.
///
/// # Behavior
/// - For each key-value pair in `required_params`, the pair is inserted into `preset`.
/// - If a key already exists in `preset`, its value will be overwritten by the value from
///   `required_params`.
/// - Both `preset` and `required_params` must be JSON dictionary objects; otherwise, the function
///   panics.
///
/// # Panics
/// This function panics if either `preset` or `required_params` is not a JSON dictionary object, or
/// if the `preset` already contains a key from the `required_params`.
fn add_required_params_to_preset(preset: &mut Value, required_params: Value) {
    if let (Value::Object(preset_map), Value::Object(required_params_map)) =
        (preset, required_params)
    {
        for (key, value) in required_params_map {
            assert!(
                !preset_map.contains_key(&key),
                "Required parameter already exists in the preset: {:?}",
                key
            );
            preset_map.insert(key, value);
        }
    } else {
        panic!("Expecting JSON object dictionary objects");
    }
}
