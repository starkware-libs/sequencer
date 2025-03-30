use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use apollo_infra_utils::test_utils::AvailablePorts;
use apollo_monitoring_endpoint::config::MonitoringEndpointConfig;
use apollo_monitoring_endpoint::test_utils::MonitoringClient;
use apollo_sequencer_node::config::component_config::ComponentConfig;
use apollo_sequencer_node::config::config_utils::{
    dump_config_file,
    BaseAppConfigOverride,
    DeploymentBaseAppConfig,
};
use apollo_sequencer_node::config::definitions::ConfigPointersMap;
use apollo_sequencer_node::config::node_config::{
    SequencerNodeConfig,
    CONFIG_NON_POINTERS_WHITELIST,
};
use apollo_sequencer_node::test_utils::node_runner::NodeRunner;
use tempfile::{tempdir, TempDir};
use tokio::fs::create_dir_all;

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
    // Client for checking liveness of the sequencer node.
    pub monitoring_client: MonitoringClient,
    // Path to the node configuration file.
    pub node_config_path: PathBuf,
    // Config values.
    pub config: SequencerNodeConfig,
    // Configuration parameters that share the same value across multiple components.
    pub config_pointers_map: ConfigPointersMap,
    // Handles for the config files, maintained so the files are not deleted. Since
    // these are only maintained to avoid dropping the handles, private visibility suffices, and
    // as such, the '#[allow(dead_code)]' attributes are used to suppress the warning.
    #[allow(dead_code)]
    node_config_dir_handle: Option<TempDir>,
}

impl ExecutableSetup {
    pub async fn new(
        mut base_app_config: DeploymentBaseAppConfig,
        config_pointers_map: ConfigPointersMap,
        node_execution_id: NodeExecutionId,
        mut available_ports: AvailablePorts,
        config_path_dir: Option<PathBuf>,
        component_config: ComponentConfig,
    ) -> Self {
        // Explicitly collect metrics in the monitoring endpoint.
        let monitoring_endpoint_config = MonitoringEndpointConfig {
            port: available_ports.get_next_port(),
            collect_metrics: true,
            ..Default::default()
        };

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

        let MonitoringEndpointConfig { ip, port, .. } = monitoring_endpoint_config;
        let monitoring_client = MonitoringClient::new(SocketAddr::from((ip, port)));

        let base_app_config_override =
            BaseAppConfigOverride::new(component_config, monitoring_endpoint_config);
        base_app_config.override_base_app_config(base_app_config_override);

        let config_path = node_config_dir.join(NODE_CONFIG_CHANGES_FILE_PATH);
        base_app_config.dump_config_file(&config_path);

        Self {
            node_execution_id,
            monitoring_client,
            config: base_app_config.get_config(),
            config_pointers_map,
            node_config_dir_handle,
            node_config_path: config_path,
        }
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

    pub fn config(&self) -> &SequencerNodeConfig {
        &self.config
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
