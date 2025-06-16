use apollo_node::config::component_config::ComponentConfig;
use apollo_node::config::component_execution_config::{
    ActiveComponentExecutionConfig,
    ReactiveComponentExecutionConfig,
};
use indexmap::IndexMap;
use serde::Serialize;
use strum::Display;
use strum_macros::{AsRefStr, EnumIter};

use crate::deployment_definitions::{Environment, EnvironmentComponentConfigModifications};
use crate::service::{
    get_ingress,
    Controller,
    GetComponentConfigs,
    Ingress,
    IngressParams,
    Resource,
    Resources,
    ServiceName,
    ServiceNameInner,
    Toleration,
};

const NODE_STORAGE: usize = 1000;

#[derive(Clone, Copy, Debug, Display, PartialEq, Eq, Hash, Serialize, AsRefStr, EnumIter)]
#[strum(serialize_all = "snake_case")]
pub enum ConsolidatedNodeServiceName {
    Node,
}

impl From<ConsolidatedNodeServiceName> for ServiceName {
    fn from(service: ConsolidatedNodeServiceName) -> Self {
        ServiceName::ConsolidatedNode(service)
    }
}

impl GetComponentConfigs for ConsolidatedNodeServiceName {
    fn get_component_configs(
        _base_port: Option<u16>,
        environment: &Environment,
    ) -> IndexMap<ServiceName, ComponentConfig> {
        let mut component_config_map = IndexMap::new();
        component_config_map.insert(
            ServiceName::ConsolidatedNode(ConsolidatedNodeServiceName::Node),
            get_consolidated_config(environment),
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
            | Environment::TestingEnvTwo
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
            | Environment::TestingEnvTwo
            | Environment::TestingEnvThree
            | Environment::StressTest => get_ingress(ingress_params, false),
            _ => unimplemented!(),
        }
    }

    fn get_storage(&self, environment: &Environment) -> Option<usize> {
        match environment {
            Environment::Testing => None,
            Environment::SepoliaIntegration
            | Environment::TestingEnvTwo
            | Environment::TestingEnvThree
            | Environment::StressTest => Some(NODE_STORAGE),
            _ => unimplemented!(),
        }
    }

    fn get_resources(&self, environment: &Environment) -> Resources {
        match environment {
            Environment::Testing => Resources::new(Resource::new(1, 2), Resource::new(4, 8)),
            Environment::SepoliaIntegration
            | Environment::TestingEnvTwo
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
            | Environment::TestingEnvTwo
            | Environment::TestingEnvThree
            | Environment::StressTest => true,
            _ => unimplemented!(),
        }
    }
}

fn get_consolidated_config(environment: &Environment) -> ComponentConfig {
    let mut base = ReactiveComponentExecutionConfig::local_with_remote_disabled();
    let EnvironmentComponentConfigModifications {
        local_server_config,
        max_concurrency,
        remote_client_config: _,
    } = environment.get_component_config_modifications();
    base.local_server_config = local_server_config;
    base.max_concurrency = max_concurrency;

    ComponentConfig {
        batcher: base.clone(),
        class_manager: base.clone(),
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
