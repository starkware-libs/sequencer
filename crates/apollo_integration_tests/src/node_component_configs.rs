use std::collections::HashMap;

use apollo_deployments::deployments::distributed::DISTRIBUTED_NODE_REQUIRED_PORTS_NUM;
use apollo_deployments::deployments::hybrid::HYBRID_NODE_REQUIRED_PORTS_NUM;
use apollo_deployments::service::{NodeService, NodeType};
use apollo_infra_utils::test_utils::AvailablePortsGenerator;
use apollo_node_config::component_config::{set_urls_to_localhost, ComponentConfig};

/// Holds the component configs for a set of sequencers, composing a single sequencer node.
pub struct NodeComponentConfigs {
    component_configs: HashMap<NodeService, ComponentConfig>,
}

impl NodeComponentConfigs {
    // TODO(victork): pass ports to the constructor and use them to get the component configs.
    fn new(component_configs: HashMap<NodeService, ComponentConfig>) -> Self {
        Self { component_configs }
    }

    pub fn len(&self) -> usize {
        self.component_configs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.component_configs.is_empty()
    }
}

impl IntoIterator for NodeComponentConfigs {
    type Item = (NodeService, ComponentConfig);
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.component_configs.into_iter().collect::<Vec<_>>().into_iter()
    }
}

pub fn create_consolidated_component_configs() -> NodeComponentConfigs {
    NodeComponentConfigs::new(NodeType::Consolidated.get_component_configs(None))
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

    NodeComponentConfigs::new(services_component_config)
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

    NodeComponentConfigs::new(services_component_config)
}
