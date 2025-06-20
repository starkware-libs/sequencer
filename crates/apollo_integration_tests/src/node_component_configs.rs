use apollo_deployments::deployments::distributed::{
    DistributedNodeServiceName,
    DISTRIBUTED_NODE_REQUIRED_PORTS_NUM,
};
use apollo_deployments::deployments::hybrid::{
    HybridNodeServiceName,
    HYBRID_NODE_REQUIRED_PORTS_NUM,
};
use apollo_deployments::service::{DeploymentName, ServiceName};
use apollo_infra_utils::test_utils::AvailablePortsGenerator;
use apollo_node::config::component_config::{set_urls_to_localhost, ComponentConfig};

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

pub fn create_consolidated_component_configs() -> NodeComponentConfigs {
    // All components are in executable index 0.
    NodeComponentConfigs::new(
        DeploymentName::ConsolidatedNode.get_component_configs(None).into_values().collect(),
        0,
        0,
        0,
        0,
    )
}

pub fn create_distributed_component_configs(
    available_ports_generator: &mut AvailablePortsGenerator,
) -> NodeComponentConfigs {
    let mut available_ports = available_ports_generator
        .next()
        .expect("Failed to get an AvailablePorts instance for distributed node configs");

    let ports = available_ports.get_next_ports(DISTRIBUTED_NODE_REQUIRED_PORTS_NUM);
    let services_component_config =
        DeploymentName::DistributedNode.get_component_configs(Some(ports));

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

pub fn create_hybrid_component_configs(
    available_ports_generator: &mut AvailablePortsGenerator,
) -> NodeComponentConfigs {
    let mut available_ports = available_ports_generator
        .next()
        .expect("Failed to get an AvailablePorts instance for distributed node configs");

    let ports = available_ports.get_next_ports(HYBRID_NODE_REQUIRED_PORTS_NUM);
    let services_component_config = DeploymentName::HybridNode.get_component_configs(Some(ports));

    let mut component_configs: Vec<ComponentConfig> =
        services_component_config.values().cloned().collect();
    set_urls_to_localhost(&mut component_configs);

    // TODO(Tsabary): transition to using the map instead of a vector and indices.

    NodeComponentConfigs::new(
        component_configs,
        services_component_config
            .get_index_of::<ServiceName>(&HybridNodeServiceName::Core.into())
            .unwrap(),
        services_component_config
            .get_index_of::<ServiceName>(&HybridNodeServiceName::HttpServer.into())
            .unwrap(),
        services_component_config
            .get_index_of::<ServiceName>(&HybridNodeServiceName::Core.into())
            .unwrap(),
        services_component_config
            .get_index_of::<ServiceName>(&HybridNodeServiceName::Core.into())
            .unwrap(),
    )
}
