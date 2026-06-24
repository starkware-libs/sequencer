use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use apollo_config::converters::{
    serialize_optional_list_with_url_and_headers,
    serialize_optional_map,
    serialize_optional_vec_u8,
    serialize_slice,
};
use apollo_monitoring_endpoint::test_utils::MonitoringClient;
use apollo_monitoring_endpoint_config::config::MonitoringEndpointConfig;
use apollo_node::test_utils::node_runner::NodeRunner;
use apollo_node_config::config_utils::DeploymentBaseAppConfig;
use apollo_node_config::definitions::ConfigPointersMap;
use apollo_node_config::node_config::SequencerNodeConfig;
use serde_json::{Map, Value};
use tempfile::{tempdir, TempDir};
use tokio::fs::create_dir_all;

const NODE_CONFIG_CHANGES_FILE_PATH: &str = "node_integration_test_config_changes.json";
const NODE_SECRETS_FILE_PATH: &str = "node_integration_test_secrets.json";

#[derive(Debug, Clone)]
pub struct NodeExecutableId {
    node_index: usize,
    node_execution_id: String,
}

impl NodeExecutableId {
    pub fn new(node_index: usize, node_execution_id: String) -> Self {
        Self { node_index, node_execution_id }
    }
    pub fn get_node_index(&self) -> usize {
        self.node_index
    }

    pub fn build_path(&self, base: &Path) -> PathBuf {
        base.join(format!("node_{}", self.node_index))
    }
}

impl std::fmt::Display for NodeExecutableId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Node id {}", self.node_index)
    }
}

impl From<NodeExecutableId> for NodeRunner {
    fn from(val: NodeExecutableId) -> Self {
        NodeRunner::new(val.node_index, val.node_execution_id)
    }
}

pub struct ExecutableSetup {
    // Node test identifier.
    pub node_executable_id: NodeExecutableId,
    // Client for checking liveness of the sequencer node.
    pub monitoring_client: MonitoringClient,
    // Path to the nested native base config file (consumed first by the native loader).
    pub node_config_path: PathBuf,
    // Path to the (empty) secrets file overlaid onto the base by the native loader.
    pub node_secrets_path: PathBuf,
    // Config.
    pub base_app_config: DeploymentBaseAppConfig,
    // Handles for the config files, maintained so the files are not deleted. Since
    // these are only maintained to avoid dropping the handles, private visibility suffices, and
    // as such, the '#[allow(dead_code)]' attributes are used to suppress the warning.
    #[allow(dead_code)]
    node_config_dir_handle: Option<TempDir>,
}

impl ExecutableSetup {
    pub async fn new(
        base_app_config: DeploymentBaseAppConfig,
        node_executable_id: NodeExecutableId,
        config_path_dir: Option<PathBuf>,
    ) -> Self {
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

        let MonitoringEndpointConfig { ip, port, .. } = base_app_config
            .config
            .monitoring_endpoint_config
            .as_ref()
            .expect("Should have a monitoring endpoint config");
        let monitoring_client = MonitoringClient::new(SocketAddr::new(*ip, *port));

        let config_path = node_config_dir.join(NODE_CONFIG_CHANGES_FILE_PATH);
        base_app_config.dump_native_config_file(&config_path);

        let secrets_path = node_config_dir.join(NODE_SECRETS_FILE_PATH);
        let secrets = build_secrets(&base_app_config.config);
        std::fs::write(&secrets_path, serde_json::to_string(&secrets).expect("Secrets serialize"))
            .expect("Should be able to write secrets file");

        Self {
            node_executable_id,
            monitoring_client,
            base_app_config,
            node_config_dir_handle,
            node_config_path: config_path,
            node_secrets_path: secrets_path,
        }
    }

    /// Config files passed to the native loader, base first then secrets, matching the
    /// `[base, secret]` arity expected by `load_native`.
    pub(crate) fn node_config_paths(&self) -> Vec<PathBuf> {
        vec![self.node_config_path.clone(), self.node_secrets_path.clone()]
    }

    pub fn modify_config<F>(&mut self, modify_config_fn: F)
    where
        F: Fn(&mut SequencerNodeConfig),
    {
        self.base_app_config.modify_config(modify_config_fn);
        self.dump_config_file_changes();
    }

    pub fn modify_config_pointers<F>(&mut self, modify_config_pointers_fn: F)
    where
        F: Fn(&mut ConfigPointersMap),
    {
        self.base_app_config.modify_config_pointers(modify_config_pointers_fn);
        self.dump_config_file_changes();
    }

    pub fn get_config(&self) -> &SequencerNodeConfig {
        self.base_app_config.get_config()
    }

    /// Re-emits the native base config file for the sequencer node after a config change.
    pub fn dump_config_file_changes(&self) {
        self.base_app_config.dump_native_config_file(&self.node_config_path);
    }
}

fn build_secrets(config: &SequencerNodeConfig) -> Value {
    let mut secrets = Map::new();

    if let Some(base_layer_config) = config.base_layer_config.as_ref() {
        let urls: Vec<&str> = base_layer_config
            .ordered_l1_endpoint_urls
            .iter()
            .map(|url| url.peek_secret().as_str())
            .collect();
        secrets.insert(
            "base_layer_config.ordered_l1_endpoint_urls".to_owned(),
            Value::String(serialize_slice(&urls)),
        );
    }

    if let Some(consensus_manager_config) = config.consensus_manager_config.as_ref() {
        let secret_key = consensus_manager_config
            .network_config
            .secret_key
            .as_ref()
            .map(|secret_key| secret_key.peek_secret().clone());
        secrets.insert(
            "consensus_manager_config.network_config.secret_key".to_owned(),
            Value::String(serialize_optional_vec_u8(&secret_key)),
        );
    }

    if let Some(l1_gas_price_provider_config) = config.l1_gas_price_provider_config.as_ref() {
        let eth_to_strk_url_header_list = l1_gas_price_provider_config
            .eth_to_strk_oracle_config
            .url_header_list
            .as_ref()
            .map(|list| list.iter().map(|item| item.peek_secret().clone()).collect::<Vec<_>>());
        secrets.insert(
            "l1_gas_price_provider_config.eth_to_strk_oracle_config.url_header_list".to_owned(),
            Value::String(serialize_optional_list_with_url_and_headers(
                &eth_to_strk_url_header_list,
            )),
        );

        let strk_to_usd_url_header_list = l1_gas_price_provider_config
            .strk_to_usd_oracle_config
            .url_header_list
            .as_ref()
            .map(|list| list.iter().map(|item| item.peek_secret().clone()).collect::<Vec<_>>());
        secrets.insert(
            "l1_gas_price_provider_config.strk_to_usd_oracle_config.url_header_list".to_owned(),
            Value::String(serialize_optional_list_with_url_and_headers(
                &strk_to_usd_url_header_list,
            )),
        );
    }

    if let Some(mempool_p2p_config) = config.mempool_p2p_config.as_ref() {
        let secret_key = mempool_p2p_config
            .network_config
            .secret_key
            .as_ref()
            .map(|secret_key| secret_key.peek_secret().clone());
        secrets.insert(
            "mempool_p2p_config.network_config.secret_key".to_owned(),
            Value::String(serialize_optional_vec_u8(&secret_key)),
        );
    }

    if let Some(state_sync_config) = config.state_sync_config.as_ref() {
        if let Some(central_sync_client_config) =
            state_sync_config.static_config.central_sync_client_config.as_ref()
        {
            let http_headers = central_sync_client_config
                .central_source_config
                .http_headers
                .as_ref()
                .map(|http_headers| http_headers.peek_secret().clone());
            secrets.insert(
                "state_sync_config.static_config.central_sync_client_config.central_source_config.\
                 http_headers"
                    .to_owned(),
                Value::String(serialize_optional_map(&http_headers)),
            );
        }

        if let Some(network_config) = state_sync_config.static_config.network_config.as_ref() {
            let secret_key = network_config
                .secret_key
                .as_ref()
                .map(|secret_key| secret_key.peek_secret().clone());
            secrets.insert(
                "state_sync_config.static_config.network_config.secret_key".to_owned(),
                Value::String(serialize_optional_vec_u8(&secret_key)),
            );
        }
    }

    Value::Object(secrets)
}
