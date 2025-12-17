use std::collections::{BTreeSet, HashMap};

use apollo_infra::component_client::remote_component_client::DEFAULT_RETRIES;
use apollo_node_config::component_config::ComponentConfig;
use apollo_node_config::component_execution_config::{
    ActiveComponentExecutionConfig,
    ReactiveComponentExecutionConfig,
};
use serde::Serialize;
use strum::{Display, IntoEnumIterator};
use strum_macros::{AsRefStr, EnumIter};

use crate::deployment_definitions::{
    BusinessLogicServicePort,
    ComponentConfigInService,
    InfraServicePort,
    ServicePort,
};
use crate::scale_policy::ScalePolicy;
use crate::service::{GetComponentConfigs, NodeService, ServiceNameInner};

#[derive(Clone, Copy, Debug, Display, PartialEq, Eq, Hash, Serialize, AsRefStr, EnumIter)]
#[strum(serialize_all = "snake_case")]
pub enum ConsolidatedNodeServiceName {
    Node,
}

impl From<ConsolidatedNodeServiceName> for NodeService {
    fn from(service: ConsolidatedNodeServiceName) -> Self {
        NodeService::Consolidated(service)
    }
}

impl GetComponentConfigs for ConsolidatedNodeServiceName {
    fn get_component_configs(_ports: Option<Vec<u16>>) -> HashMap<NodeService, ComponentConfig> {
        let mut component_config_map = HashMap::new();
        component_config_map.insert(
            NodeService::Consolidated(ConsolidatedNodeServiceName::Node),
            get_consolidated_config(),
        );
        component_config_map
    }
}

impl ServiceNameInner for ConsolidatedNodeServiceName {
    fn get_scale_policy(&self) -> ScalePolicy {
        match self {
            ConsolidatedNodeServiceName::Node => ScalePolicy::StaticallyScaled,
        }
    }

    fn get_retries(&self) -> usize {
        match self {
            Self::Node => DEFAULT_RETRIES,
        }
    }

    fn get_service_ports(&self) -> BTreeSet<ServicePort> {
        let mut service_ports = BTreeSet::new();
        for service_port in ServicePort::iter() {
            match service_port {
                ServicePort::BusinessLogic(bl_port) => match bl_port {
                    BusinessLogicServicePort::MonitoringEndpoint
                    | BusinessLogicServicePort::HttpServer
                    | BusinessLogicServicePort::ConsensusP2p
                    | BusinessLogicServicePort::MempoolP2p => {
                        service_ports.insert(service_port);
                    }
                },
                ServicePort::Infra(infra_port) => match infra_port {
                    InfraServicePort::Batcher
                    | InfraServicePort::Mempool
                    | InfraServicePort::ClassManager
                    | InfraServicePort::Gateway
                    | InfraServicePort::L1GasPriceProvider
                    | InfraServicePort::L1Provider
                    | InfraServicePort::SierraCompiler
                    | InfraServicePort::StateSync
                    | InfraServicePort::SignatureManager => {}
                },
            }
        }

        service_ports
    }

    fn get_components_in_service(&self) -> BTreeSet<ComponentConfigInService> {
        match self {
            ConsolidatedNodeServiceName::Node => ComponentConfigInService::iter().collect(),
        }
    }
}

fn get_consolidated_config() -> ComponentConfig {
    let base = ReactiveComponentExecutionConfig::local_with_remote_disabled();

    ComponentConfig {
        batcher: base.clone(),
        class_manager: base.clone(),
        config_manager: base.clone(),
        consensus_manager: ActiveComponentExecutionConfig::enabled(),
        gateway: base.clone(),
        http_server: ActiveComponentExecutionConfig::enabled(),
        l1_provider: base.clone(),
        l1_scraper: ActiveComponentExecutionConfig::enabled(),
        l1_gas_price_provider: base.clone(),
        l1_gas_price_scraper: ActiveComponentExecutionConfig::enabled(),
        mempool: base.clone(),
        mempool_p2p: base.clone(),
        monitoring_endpoint: ActiveComponentExecutionConfig::enabled(),
        sierra_compiler: base.clone(),
        signature_manager: base.clone(),
        state_sync: base.clone(),
    }
}
