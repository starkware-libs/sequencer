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
    Controller,
    ExternalSecret,
    GetComponentConfigs,
    Ingress,
    IngressRule,
    Resource,
    Resources,
    Service,
    ServiceName,
    ServiceNameInner,
};

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
    fn create_service(&self) -> Service {
        match self {
            ConsolidatedNodeServiceName::Node => Service::new(
                Into::<ServiceName>::into(*self),
                Controller::StatefulSet,
                Ingress::new(
                    String::from("sw-dev.io"),
                    true,
                    vec![
                        IngressRule::new(String::from("/gateway"), 8080),
                        IngressRule::new(String::from("/feeder-gateway"), 8080),
                    ],
                    vec![],
                ),
                false,
                1,
                Some(32),
                None,
                Resources::new(Resource::new(1, 2), Resource::new(4, 8)),
                Some(ExternalSecret::new("sequencer-dev-secrets")),
            ),
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
        gateway: base.clone(),
        mempool: base.clone(),
        mempool_p2p: base.clone(),
        sierra_compiler: base.clone(),
        state_sync: base.clone(),
        l1_provider: base.clone(),
        l1_gas_price_provider: base.clone(),
        consensus_manager: ActiveComponentExecutionConfig::enabled(),
        http_server: ActiveComponentExecutionConfig::enabled(),
        l1_scraper: ActiveComponentExecutionConfig::enabled(),
        l1_gas_price_scraper: ActiveComponentExecutionConfig::enabled(),
        monitoring_endpoint: ActiveComponentExecutionConfig::enabled(),
    }
}
