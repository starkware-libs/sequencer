use apollo_deployments::deployments::consolidated::ConsolidatedNodeServiceName;
use apollo_deployments::deployments::distributed::{
    DistributedNodeServiceName,
    DISTRIBUTED_NODE_REQUIRED_PORTS_NUM,
};
use apollo_deployments::deployments::hybrid::{
    HybridNodeServiceName,
    HYBRID_NODE_REQUIRED_PORTS_NUM,
};
use apollo_deployments::service::{NodeService, NodeType};
use apollo_infra_utils::test_utils::AvailablePortsGenerator;
use apollo_node::config::component_config::{set_urls_to_localhost, ComponentConfig};
use indexmap::map::IntoValues;
use indexmap::IndexMap;

/// Holds the component configs for a set of sequencers, composing a single sequencer node.
pub struct NodeComponentConfigs {
    component_configs: IndexMap<NodeService, ComponentConfig>,
    batcher_service: NodeService,
    http_server_service: NodeService,
    state_sync_service: NodeService,
    class_manager_service: NodeService,
    consensus_manager_service: NodeService,
}

impl NodeComponentConfigs {
    fn new(
        component_configs: IndexMap<NodeService, ComponentConfig>,
        batcher_service: NodeService,
        http_server_service: NodeService,
        state_sync_service: NodeService,
        class_manager_service: NodeService,
        consensus_manager_service: NodeService,
    ) -> Self {
        Self {
            component_configs,
            batcher_service,
            http_server_service,
            state_sync_service,
            class_manager_service,
            consensus_manager_service,
        }
    }

    pub fn len(&self) -> usize {
        self.component_configs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.component_configs.is_empty()
    }

    pub fn get_batcher_service(&self) -> NodeService {
        self.batcher_service
    }

    pub fn get_http_server_service(&self) -> NodeService {
        self.http_server_service
    }

    pub fn get_state_sync_service(&self) -> NodeService {
        self.state_sync_service
    }

    pub fn get_class_manager_service(&self) -> NodeService {
        self.class_manager_service
    }

    pub fn get_consensus_manager_service(&self) -> NodeService {
        self.consensus_manager_service
    }
}

impl IntoIterator for NodeComponentConfigs {
    type Item = ComponentConfig;
    type IntoIter = IntoValues<NodeService, ComponentConfig>;

    fn into_iter(self) -> Self::IntoIter {
        self.component_configs.into_values()
    }
}

pub fn create_consolidated_component_configs() -> NodeComponentConfigs {
    // All components are in executable index 0.
    NodeComponentConfigs::new(
        NodeType::Consolidated.get_component_configs(None),
        ConsolidatedNodeServiceName::Node.into(),
        ConsolidatedNodeServiceName::Node.into(),
        ConsolidatedNodeServiceName::Node.into(),
        ConsolidatedNodeServiceName::Node.into(),
        ConsolidatedNodeServiceName::Node.into(),
    )
}

pub fn create_distributed_component_configs(
    available_ports_generator: &mut AvailablePortsGenerator,
) -> NodeComponentConfigs {
    let mut available_ports = available_ports_generator
        .next()
        .expect("Failed to get an AvailablePorts instance for distributed node configs");

    let ports = available_ports.get_next_ports(DISTRIBUTED_NODE_REQUIRED_PORTS_NUM);
    let mut services_component_config = NodeType::Distributed.get_component_configs(Some(ports));

    set_urls_to_localhost(services_component_config.values_mut());

    NodeComponentConfigs::new(
        services_component_config,
        DistributedNodeServiceName::Batcher.into(),
        DistributedNodeServiceName::HttpServer.into(),
        DistributedNodeServiceName::StateSync.into(),
        DistributedNodeServiceName::ClassManager.into(),
        DistributedNodeServiceName::ConsensusManager.into(),
    )
}

pub fn create_hybrid_component_configs(
    available_ports_generator: &mut AvailablePortsGenerator,
) -> NodeComponentConfigs {
    let mut available_ports = available_ports_generator
        .next()
        .expect("Failed to get an AvailablePorts instance for distributed node configs");

    let ports = available_ports.get_next_ports(HYBRID_NODE_REQUIRED_PORTS_NUM);
    let mut services_component_config = NodeType::Hybrid.get_component_configs(Some(ports));

    set_urls_to_localhost(services_component_config.values_mut());

    NodeComponentConfigs::new(
        services_component_config,
        HybridNodeServiceName::Core.into(),
        HybridNodeServiceName::HttpServer.into(),
        HybridNodeServiceName::Core.into(),
        HybridNodeServiceName::Core.into(),
        HybridNodeServiceName::Core.into(),
    )
}
