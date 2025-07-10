use std::collections::BTreeMap;

use apollo_monitoring_endpoint::config::MONITORING_ENDPOINT_DEFAULT_PORT;
use apollo_node::config::component_config::ComponentConfig;
use apollo_node::config::component_execution_config::{
    ActiveComponentExecutionConfig,
    ReactiveComponentExecutionConfig,
};
use indexmap::IndexMap;
use serde::Serialize;
use strum::Display;
use strum_macros::{AsRefStr, EnumIter};

use crate::deployment_definitions::{Environment, ServicePort};
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
            Environment::Testing => None,
            Environment::SepoliaIntegration
            | Environment::SepoliaTestnet
            | Environment::UpgradeTest
            | Environment::TestingEnvThree
            | Environment::StressTest => match self {
                ConsolidatedNodeServiceName::Node => Some(Toleration::ApolloCoreService),
            },
            _ => unimplemented!(),
        }
    }

    fn get_ingress(
        &self,
        environment: &Environment,
        ingress_params: IngressParams,
    ) -> Option<Ingress> {
        match environment {
            Environment::Testing => None,
            Environment::SepoliaIntegration
            | Environment::SepoliaTestnet
            | Environment::UpgradeTest
            | Environment::TestingEnvThree
            | Environment::StressTest => get_ingress(ingress_params, false),
            _ => unimplemented!(),
        }
    }

    fn has_p2p_interface(&self) -> bool {
        true
    }

    fn get_storage(&self, environment: &Environment) -> Option<usize> {
        match environment {
            Environment::Testing => Some(TESTING_NODE_STORAGE),
            Environment::SepoliaIntegration
            | Environment::SepoliaTestnet
            | Environment::UpgradeTest
            | Environment::TestingEnvThree
            | Environment::StressTest => Some(NODE_STORAGE),
            _ => unimplemented!(),
        }
    }

    fn get_resources(&self, environment: &Environment) -> Resources {
        match environment {
            Environment::Testing => Resources::new(Resource::new(1, 2), Resource::new(4, 8)),
            Environment::SepoliaIntegration
            | Environment::SepoliaTestnet
            | Environment::UpgradeTest
            | Environment::TestingEnvThree
            | Environment::StressTest => Resources::new(Resource::new(2, 4), Resource::new(4, 8)),
            _ => unimplemented!(),
        }
    }

    fn get_replicas(&self, _environment: &Environment) -> usize {
        1
    }

    fn get_anti_affinity(&self, environment: &Environment) -> bool {
        match environment {
            Environment::Testing => false,
            Environment::SepoliaIntegration
            | Environment::SepoliaTestnet
            | Environment::UpgradeTest
            | Environment::TestingEnvThree
            | Environment::StressTest => true,
            _ => unimplemented!(),
        }
    }

    // TODO(Nadin): Implement this method to return the actual ports used by the service.
    fn get_ports(&self) -> BTreeMap<ServicePort, u16> {
        let mut ports = BTreeMap::new();
        ports.insert(ServicePort::MonitoringEndpoint, MONITORING_ENDPOINT_DEFAULT_PORT);
        ports
    }
}

fn get_consolidated_config() -> ComponentConfig {
    let base = ReactiveComponentExecutionConfig::local_with_remote_disabled();

    ComponentConfig {
        batcher: base.clone(),
        class_manager: base.clone(),
        gateway: base.clone(),
        mempool: base.clone(),
        mempool_p2p: base.clone(),
        sierra_compiler: base.clone(),
        state_sync: base.clone(),
        l1_endpoint_monitor: base.clone(),
        l1_provider: base.clone(),
        l1_gas_price_provider: base.clone(),
        consensus_manager: ActiveComponentExecutionConfig::enabled(),
        http_server: ActiveComponentExecutionConfig::enabled(),
        l1_scraper: ActiveComponentExecutionConfig::enabled(),
        l1_gas_price_scraper: ActiveComponentExecutionConfig::enabled(),
        monitoring_endpoint: ActiveComponentExecutionConfig::enabled(),
    }
}
