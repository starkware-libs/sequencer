use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use apollo_monitoring_endpoint::test_utils::MonitoringClient;
use apollo_monitoring_endpoint_config::config::MonitoringEndpointConfig;
use apollo_node::test_utils::node_runner::NodeRunner;
use apollo_node_config::config_utils::DeploymentBaseAppConfig;
use apollo_node_config::definitions::ConfigPointersMap;
use apollo_node_config::node_config::SequencerNodeConfig;
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

        // The native loader requires a secrets file overlaid onto the base. The harness carries no
        // secrets, so emit an empty JSON object.
        let secrets_path = node_config_dir.join(NODE_SECRETS_FILE_PATH);
        std::fs::write(&secrets_path, "{}").expect("Should be able to write secrets file");

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
    pub fn node_config_paths(&self) -> Vec<PathBuf> {
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
