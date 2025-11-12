use std::collections::HashMap;

use apollo_deployments::deployments::distributed::DISTRIBUTED_NODE_REQUIRED_PORTS_NUM;
use apollo_deployments::deployments::hybrid::HYBRID_NODE_REQUIRED_PORTS_NUM;
use apollo_deployments::service::{NodeService, NodeType};
use apollo_infra_utils::test_utils::AvailablePortsGenerator;
use apollo_node_config::component_config::{set_urls_to_localhost, ComponentConfig};

pub type NodeComponentConfigs = HashMap<NodeService, ComponentConfig>;

pub fn create_consolidated_component_configs() -> NodeComponentConfigs {
    NodeType::Consolidated.get_component_configs(None)
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

    services_component_config
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

    services_component_config
}
