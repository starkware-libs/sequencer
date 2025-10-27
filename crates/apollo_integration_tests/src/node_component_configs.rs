use apollo_deployments::deployment_definitions::ComponentConfigInService;
use apollo_deployments::deployments::distributed::DISTRIBUTED_NODE_REQUIRED_PORTS_NUM;
use apollo_deployments::deployments::hybrid::HYBRID_NODE_REQUIRED_PORTS_NUM;
use apollo_deployments::service::{NodeService, NodeType};
use apollo_infra_utils::test_utils::AvailablePortsGenerator;
use apollo_node_config::component_config::{set_urls_to_localhost, ComponentConfig};
use indexmap::IndexMap;

/// Holds the component configs for a set of sequencers, composing a single sequencer node.
pub struct NodeComponentConfigs {
    // TODO(Tsabary): transition to using the map instead of a vector and indices.
    component_configs: Vec<ComponentConfig>,
    batcher_index: usize,
    http_server_index: usize,
    state_sync_index: usize,
    class_manager_index: usize,
    consensus_manager_index: usize,
}

impl NodeComponentConfigs {
    // TODO(victork): only pass node_type and ports, and create the map inside.
    fn new(component_configs: IndexMap<NodeService, ComponentConfig>, node_type: NodeType) -> Self {
        fn get_component_index(
            component_configs: &IndexMap<NodeService, ComponentConfig>,
            node_type: NodeType,
            component_in_service: ComponentConfigInService,
        ) -> usize {
            component_configs
                .get_index_of::<NodeService>(
                    node_type
                        .get_services_of_components(component_in_service)
                        .iter()
                        .next()
                        .as_ref()
                        .unwrap(),
                )
                .unwrap()
        }

        let batcher_index =
            get_component_index(&component_configs, node_type, ComponentConfigInService::Batcher);

        let http_server_index = get_component_index(
            &component_configs,
            node_type,
            ComponentConfigInService::HttpServer,
        );

        let state_sync_index =
            get_component_index(&component_configs, node_type, ComponentConfigInService::StateSync);

        let class_manager_index = get_component_index(
            &component_configs,
            node_type,
            ComponentConfigInService::ClassManager,
        );

        let consensus_manager_index = get_component_index(
            &component_configs,
            node_type,
            ComponentConfigInService::ConsensusManager,
        );

        Self {
            component_configs: component_configs.into_values().collect(),
            batcher_index,
            http_server_index,
            state_sync_index,
            class_manager_index,
            consensus_manager_index,
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

    pub fn get_consensus_manager_index(&self) -> usize {
        self.consensus_manager_index
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
        NodeType::Consolidated.get_component_configs(None),
        NodeType::Consolidated,
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

    NodeComponentConfigs::new(services_component_config, NodeType::Distributed)
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

    NodeComponentConfigs::new(services_component_config, NodeType::Hybrid)
}
