use apollo_infra_utils::test_utils::AvailablePortsGenerator;
use apollo_sequencer_deployments::deployments::distributed::DistributedNodeServiceName;
use apollo_sequencer_deployments::service::{DeploymentName, ServiceName};
use apollo_sequencer_node::config::component_config::{set_urls_to_localhost, ComponentConfig};

/// Holds the component configs for a set of sequencers, composing a single sequencer node.
pub struct NodeComponentConfigs {
    component_configs: Vec<ComponentConfig>,
    batcher_index: usize,
    http_server_index: usize,
    state_sync_index: usize,
    class_manager_index: usize,
}

impl NodeComponentConfigs {
    fn new(
        component_configs: Vec<ComponentConfig>,
        batcher_index: usize,
        http_server_index: usize,
        state_sync_index: usize,
        class_manager_index: usize,
    ) -> Self {
        Self {
            component_configs,
            batcher_index,
            http_server_index,
            state_sync_index,
            class_manager_index,
        }
    }

    pub fn len(&self) -> usize {
        self.component_configs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.component_configs.is_empty()
    }

    pub fn get_batcher_index(&self) -> usize {
        self.batcher_index
    }

    pub fn get_http_server_index(&self) -> usize {
        self.http_server_index
    }

    pub fn get_state_sync_index(&self) -> usize {
        self.state_sync_index
    }

    pub fn get_class_manager_index(&self) -> usize {
        self.class_manager_index
    }
}

impl IntoIterator for NodeComponentConfigs {
    type Item = ComponentConfig;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.component_configs.into_iter()
    }
}

pub fn create_consolidated_sequencer_configs() -> NodeComponentConfigs {
    // All components are in executable index 0.
    NodeComponentConfigs::new(
        DeploymentName::ConsolidatedNode.get_component_configs(None).into_values().collect(),
        0,
        0,
        0,
        0,
    )
}

pub fn create_nodes_deployment_units_configs(
    available_ports_generator: &mut AvailablePortsGenerator,
) -> NodeComponentConfigs {
    let mut available_ports = available_ports_generator
        .next()
        .expect("Failed to get an AvailablePorts instance for distributed node configs");

    // TODO(Tsabary): the following implicitly assumes there are sufficiently many ports
    // available in the [`available_ports`] instance to support the deployment configuration. If
    // the test breaks due to port binding conflicts then it might be required to revisit this
    // assumption.

    let base_port = available_ports.get_next_port();

    let services_component_config =
        DeploymentName::DistributedNode.get_component_configs(Some(base_port));

    let mut component_configs: Vec<ComponentConfig> =
        services_component_config.values().cloned().collect();
    set_urls_to_localhost(&mut component_configs);

    // TODO(Tsabary): transition to using the map instead of a vector and indices.

    NodeComponentConfigs::new(
        component_configs,
        services_component_config
            .get_index_of::<ServiceName>(&DistributedNodeServiceName::Batcher.into())
            .unwrap(),
        services_component_config
            .get_index_of::<ServiceName>(&DistributedNodeServiceName::HttpServer.into())
            .unwrap(),
        services_component_config
            .get_index_of::<ServiceName>(&DistributedNodeServiceName::StateSync.into())
            .unwrap(),
        services_component_config
            .get_index_of::<ServiceName>(&DistributedNodeServiceName::ClassManager.into())
            .unwrap(),
    )
}
