use std::collections::BTreeSet;

use apollo_node::config::component_config::ComponentConfig;
use apollo_node::config::component_execution_config::{
    ActiveComponentExecutionConfig,
    ReactiveComponentExecutionConfig,
};
use indexmap::IndexMap;
use serde::Serialize;
use strum::{Display, IntoEnumIterator};
use strum_macros::{AsRefStr, EnumIter};

use crate::deployment_definitions::{
    BusinessLogicServicePort,
    ComponentConfigInService,
    Environment,
    InfraServicePort,
    ServicePort,
};
use crate::k8s::{
    get_ingress,
    Controller,
    Ingress,
    IngressParams,
    Resource,
    Resources,
    Toleration,
};
use crate::service::{GetComponentConfigs, NodeService, ServiceNameInner};
use crate::update_strategy::UpdateStrategy;

const NODE_STORAGE: usize = 1000;
const TESTING_NODE_STORAGE: usize = 1;

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
    fn get_component_configs(_ports: Option<Vec<u16>>) -> IndexMap<NodeService, ComponentConfig> {
        let mut component_config_map = IndexMap::new();
        component_config_map.insert(
            NodeService::Consolidated(ConsolidatedNodeServiceName::Node),
            get_consolidated_config(),
        );
        component_config_map
    }
}

impl ServiceNameInner for ConsolidatedNodeServiceName {
    fn get_controller(&self) -> Controller {
        match self {
            ConsolidatedNodeServiceName::Node => Controller::StatefulSet,
        }
    }

    fn get_autoscale(&self) -> bool {
        match self {
            ConsolidatedNodeServiceName::Node => false,
        }
    }

    fn get_toleration(&self, environment: &Environment) -> Option<Toleration> {
        match environment {
            Environment::CloudK8s(_) => Some(Toleration::ApolloCoreService),
            Environment::LocalK8s => None,
        }
    }

    fn get_ingress(
        &self,
        environment: &Environment,
        ingress_params: IngressParams,
    ) -> Option<Ingress> {
        match environment {
            Environment::CloudK8s(_) => get_ingress(ingress_params, false),
            Environment::LocalK8s => None,
        }
    }

    fn has_p2p_interface(&self) -> bool {
        true
    }

    fn get_storage(&self, environment: &Environment) -> Option<usize> {
        match environment {
            Environment::CloudK8s(_) => Some(NODE_STORAGE),
            Environment::LocalK8s => Some(TESTING_NODE_STORAGE),
        }
    }

    fn get_resources(&self, environment: &Environment) -> Resources {
        match environment {
            Environment::CloudK8s(_) => Resources::new(Resource::new(2, 4), Resource::new(4, 8)),
            Environment::LocalK8s => Resources::new(Resource::new(1, 2), Resource::new(4, 8)),
        }
    }

    fn get_replicas(&self, _environment: &Environment) -> usize {
        1
    }

    fn get_anti_affinity(&self, environment: &Environment) -> bool {
        match environment {
            Environment::CloudK8s(_) => true,
            Environment::LocalK8s => false,
        }
    }

<<<<<<< HEAD
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
                    | InfraServicePort::L1EndpointMonitor
                    | InfraServicePort::L1GasPriceProvider
                    | InfraServicePort::L1Provider
                    | InfraServicePort::SierraCompiler
                    | InfraServicePort::StateSync
                    | InfraServicePort::SignatureManager => {}
                },
            }
        }

        service_ports
||||||| 38f03e1d0
    // TODO(Nadin): Implement this method to return the actual ports used by the service.
    fn get_ports(&self) -> BTreeMap<ServicePort, u16> {
        BTreeMap::new()
=======
    fn get_service_ports(&self) -> BTreeSet<ServicePort> {
        let mut service_ports = BTreeSet::new();
        for service_port in ServicePort::iter() {
            match service_port {
                ServicePort::BusinessLogic(bl_port) => match bl_port {
                    BusinessLogicServicePort::MonitoringEndpoint
                    | BusinessLogicServicePort::HttpServer
                    | BusinessLogicServicePort::ConsensusP2P
                    | BusinessLogicServicePort::MempoolP2p => {
                        service_ports.insert(service_port);
                    }
                },
                ServicePort::Infra(infra_port) => match infra_port {
                    InfraServicePort::Batcher
                    | InfraServicePort::Mempool
                    | InfraServicePort::ClassManager
                    | InfraServicePort::Gateway
                    | InfraServicePort::L1EndpointMonitor
                    | InfraServicePort::L1GasPriceProvider
                    | InfraServicePort::L1Provider
                    | InfraServicePort::SierraCompiler
                    | InfraServicePort::StateSync => {}
                },
            }
        }

        service_ports
>>>>>>> origin/main-v0.14.0
    }

    fn get_components_in_service(&self) -> BTreeSet<ComponentConfigInService> {
        match self {
            ConsolidatedNodeServiceName::Node => ComponentConfigInService::iter().collect(),
        }
    }

    fn get_update_strategy(&self) -> UpdateStrategy {
        match self {
            ConsolidatedNodeServiceName::Node => UpdateStrategy::Recreate,
        }
    }
}

fn get_consolidated_config() -> ComponentConfig {
    let base = ReactiveComponentExecutionConfig::local_with_remote_disabled();

    ComponentConfig {
        batcher: base.clone(),
        class_manager: base.clone(),
        consensus_manager: ActiveComponentExecutionConfig::enabled(),
        gateway: base.clone(),
        http_server: ActiveComponentExecutionConfig::enabled(),
        l1_endpoint_monitor: base.clone(),
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
