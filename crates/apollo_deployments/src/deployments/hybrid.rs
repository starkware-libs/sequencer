use std::net::{IpAddr, Ipv4Addr};

use apollo_node::config::component_config::ComponentConfig;
use apollo_node::config::component_execution_config::{
    ActiveComponentExecutionConfig,
    ReactiveComponentExecutionConfig,
};
use indexmap::IndexMap;
use serde::Serialize;
use strum::{Display, IntoEnumIterator};
use strum_macros::{AsRefStr, EnumIter};

use crate::service::{
    GetComponentConfigs,
    Resource,
    Resources,
    Service,
    ServiceName,
    ServiceNameInner,
};

const BASE_PORT: u16 = 55000; // TODO(Tsabary): arbitrary port, need to resolve.

#[repr(u16)]
#[derive(Clone, Copy, Debug, Display, PartialEq, Eq, Hash, Serialize, AsRefStr, EnumIter)]
#[strum(serialize_all = "snake_case")]
pub enum HybridNodeServiceName {
    Batcher,
    ClassManager,
    ConsensusManager,
    HttpServer,
    Gateway,
    L1,
    Mempool,
    SierraCompiler,
    StateSync,
}

// Implement conversion from `HybridNodeServiceName` to `ServiceName`
impl From<HybridNodeServiceName> for ServiceName {
    fn from(service: HybridNodeServiceName) -> Self {
        ServiceName::HybridNode(service)
    }
}

impl GetComponentConfigs for HybridNodeServiceName {
    fn get_component_configs(base_port: Option<u16>) -> IndexMap<ServiceName, ComponentConfig> {
        let mut component_config_map = IndexMap::<ServiceName, ComponentConfig>::new();

        // TODO(Tsabary): the following is a temporary solution to differentiate the l1 provider
        // and the l1 gas price provider ports. Need to come up with a better way for that.
        // The offset value has to exceed 3, to avoid conflicting with the remaining services:
        // mempool, sierra compiler, and state sync. The value of 5 was chosen arbitrarily
        // to satisfy the above.
        let base_port_with_offset = Some(base_port.unwrap_or(BASE_PORT) + 5);

        let batcher = HybridNodeServiceName::Batcher.component_config_pair(base_port);
        let class_manager = HybridNodeServiceName::ClassManager.component_config_pair(base_port);
        let gateway = HybridNodeServiceName::Gateway.component_config_pair(base_port);
        let l1_gas_price_provider =
            HybridNodeServiceName::L1.component_config_pair(base_port_with_offset);
        let l1_provider = HybridNodeServiceName::L1.component_config_pair(base_port);
        let mempool = HybridNodeServiceName::Mempool.component_config_pair(base_port);
        let sierra_compiler =
            HybridNodeServiceName::SierraCompiler.component_config_pair(base_port);
        let state_sync = HybridNodeServiceName::StateSync.component_config_pair(base_port);

        for inner_service_name in HybridNodeServiceName::iter() {
            let component_config = match inner_service_name {
                HybridNodeServiceName::Batcher => get_batcher_component_config(
                    batcher.local(),
                    class_manager.remote(),
                    l1_provider.remote(),
                    mempool.remote(),
                ),
                HybridNodeServiceName::ClassManager => get_class_manager_component_config(
                    class_manager.local(),
                    sierra_compiler.remote(),
                ),
                HybridNodeServiceName::ConsensusManager => get_consensus_manager_component_config(
                    batcher.remote(),
                    class_manager.remote(),
                    l1_gas_price_provider.remote(),
                    state_sync.remote(),
                ),
                HybridNodeServiceName::HttpServer => {
                    get_http_server_component_config(gateway.remote())
                }
                HybridNodeServiceName::Gateway => get_gateway_component_config(
                    gateway.local(),
                    class_manager.remote(),
                    mempool.remote(),
                    state_sync.remote(),
                ),
                HybridNodeServiceName::L1 => get_l1_component_config(
                    l1_gas_price_provider.local(),
                    l1_provider.local(),
                    state_sync.remote(),
                ),
                HybridNodeServiceName::Mempool => get_mempool_component_config(
                    mempool.local(),
                    class_manager.remote(),
                    gateway.remote(),
                ),
                HybridNodeServiceName::SierraCompiler => {
                    get_sierra_compiler_component_config(sierra_compiler.local())
                }
                HybridNodeServiceName::StateSync => {
                    get_state_sync_component_config(state_sync.local(), class_manager.remote())
                }
            };
            let service_name = inner_service_name.into();
            component_config_map.insert(service_name, component_config);
        }
        component_config_map
    }
}

// TODO(Tsabary): per each service, update all values.
impl ServiceNameInner for HybridNodeServiceName {
    fn create_service(&self) -> Service {
        match self {
            HybridNodeServiceName::Batcher => Service::new(
                Into::<ServiceName>::into(*self),
                false,
                false,
                1,
                Some(32),
                Resources::new(Resource::new(1, 2), Resource::new(4, 8)),
                None,
            ),
            HybridNodeServiceName::ClassManager => Service::new(
                Into::<ServiceName>::into(*self),
                false,
                false,
                1,
                Some(32),
                Resources::new(Resource::new(1, 2), Resource::new(4, 8)),
                None,
            ),
            HybridNodeServiceName::ConsensusManager => Service::new(
                Into::<ServiceName>::into(*self),
                false,
                false,
                1,
                None,
                Resources::new(Resource::new(1, 2), Resource::new(4, 8)),
                None,
            ),
            HybridNodeServiceName::HttpServer => Service::new(
                Into::<ServiceName>::into(*self),
                false,
                false,
                1,
                None,
                Resources::new(Resource::new(1, 2), Resource::new(4, 8)),
                None,
            ),
            HybridNodeServiceName::Gateway => Service::new(
                Into::<ServiceName>::into(*self),
                false,
                false,
                1,
                None,
                Resources::new(Resource::new(1, 2), Resource::new(4, 8)),
                None,
            ),
            HybridNodeServiceName::L1 => Service::new(
                Into::<ServiceName>::into(*self),
                false,
                false,
                1,
                None,
                Resources::new(Resource::new(1, 2), Resource::new(4, 8)),
                None,
            ),
            HybridNodeServiceName::Mempool => Service::new(
                Into::<ServiceName>::into(*self),
                false,
                false,
                1,
                None,
                Resources::new(Resource::new(1, 2), Resource::new(4, 8)),
                None,
            ),
            HybridNodeServiceName::SierraCompiler => Service::new(
                Into::<ServiceName>::into(*self),
                false,
                false,
                1,
                None,
                Resources::new(Resource::new(1, 2), Resource::new(4, 8)),
                None,
            ),
            HybridNodeServiceName::StateSync => Service::new(
                Into::<ServiceName>::into(*self),
                false,
                false,
                1,
                Some(32),
                Resources::new(Resource::new(1, 2), Resource::new(4, 8)),
                None,
            ),
        }
    }
}

impl HybridNodeServiceName {
    /// Returns a component execution config for a component that runs locally, and accepts inbound
    /// connections from remote components.
    pub fn component_config_for_local_service(
        &self,
        base_port: Option<u16>,
    ) -> ReactiveComponentExecutionConfig {
        ReactiveComponentExecutionConfig::local_with_remote_enabled(
            self.url(),
            self.ip(),
            self.port(base_port),
        )
    }

    /// Returns a component execution config for a component that is accessed remotely.
    pub fn component_config_for_remote_service(
        &self,
        base_port: Option<u16>,
    ) -> ReactiveComponentExecutionConfig {
        ReactiveComponentExecutionConfig::remote(self.url(), self.ip(), self.port(base_port))
    }

    fn component_config_pair(&self, base_port: Option<u16>) -> HybridNodeServiceConfigPair {
        HybridNodeServiceConfigPair {
            local: self.component_config_for_local_service(base_port),
            remote: self.component_config_for_remote_service(base_port),
        }
    }

    /// Url for the service.
    fn url(&self) -> String {
        // This must match the Kubernetes service name as defined by CDK8s.
        let formatted_service_name = self.as_ref().replace('_', "");
        format!("sequencer-{}-service", formatted_service_name)
    }

    /// Unique port number per service.
    fn port(&self, base_port: Option<u16>) -> u16 {
        let port_offset = self.get_port_offset();
        let base_port = base_port.unwrap_or(BASE_PORT);
        base_port + port_offset
    }

    /// Listening address per service.
    fn ip(&self) -> IpAddr {
        IpAddr::from(Ipv4Addr::UNSPECIFIED)
    }

    // Use the enum discriminant to generate a unique port per service.
    // TODO(Tsabary): consider alternatives that enable removing the linter suppression.
    #[allow(clippy::as_conversions)]
    fn get_port_offset(&self) -> u16 {
        *self as u16
    }
}

/// Component config bundling for services of a hybrid node: a config to run a component
/// locally while being accessible to other services, and a suitable config enabling such services
/// the access.
struct HybridNodeServiceConfigPair {
    local: ReactiveComponentExecutionConfig,
    remote: ReactiveComponentExecutionConfig,
}

impl HybridNodeServiceConfigPair {
    fn local(&self) -> ReactiveComponentExecutionConfig {
        self.local.clone()
    }

    fn remote(&self) -> ReactiveComponentExecutionConfig {
        self.remote.clone()
    }
}

fn get_batcher_component_config(
    batcher_local_config: ReactiveComponentExecutionConfig,
    class_manager_remote_config: ReactiveComponentExecutionConfig,
    l1_provider_remote_config: ReactiveComponentExecutionConfig,
    mempool_remote_config: ReactiveComponentExecutionConfig,
) -> ComponentConfig {
    let mut config = ComponentConfig::disabled();
    config.batcher = batcher_local_config;
    config.class_manager = class_manager_remote_config;
    config.l1_provider = l1_provider_remote_config;
    config.mempool = mempool_remote_config;
    config.monitoring_endpoint = ActiveComponentExecutionConfig::enabled();
    config
}

fn get_class_manager_component_config(
    class_manager_local_config: ReactiveComponentExecutionConfig,
    sierra_compiler_remote_config: ReactiveComponentExecutionConfig,
) -> ComponentConfig {
    let mut config = ComponentConfig::disabled();
    config.class_manager = class_manager_local_config;
    config.sierra_compiler = sierra_compiler_remote_config;
    config.monitoring_endpoint = ActiveComponentExecutionConfig::enabled();
    config
}

fn get_gateway_component_config(
    gateway_local_config: ReactiveComponentExecutionConfig,
    class_manager_remote_config: ReactiveComponentExecutionConfig,
    mempool_remote_config: ReactiveComponentExecutionConfig,
    state_sync_remote_config: ReactiveComponentExecutionConfig,
) -> ComponentConfig {
    let mut config = ComponentConfig::disabled();
    config.gateway = gateway_local_config;
    config.class_manager = class_manager_remote_config;
    config.mempool = mempool_remote_config;
    config.state_sync = state_sync_remote_config;
    config.monitoring_endpoint = ActiveComponentExecutionConfig::enabled();
    config
}

fn get_mempool_component_config(
    mempool_local_config: ReactiveComponentExecutionConfig,
    class_manager_remote_config: ReactiveComponentExecutionConfig,
    gateway_remote_config: ReactiveComponentExecutionConfig,
) -> ComponentConfig {
    let mut config = ComponentConfig::disabled();
    config.mempool = mempool_local_config;
    config.mempool_p2p = ReactiveComponentExecutionConfig::local_with_remote_disabled();
    config.class_manager = class_manager_remote_config;
    config.gateway = gateway_remote_config;
    config.monitoring_endpoint = ActiveComponentExecutionConfig::enabled();
    config
}

fn get_sierra_compiler_component_config(
    sierra_compiler_local_config: ReactiveComponentExecutionConfig,
) -> ComponentConfig {
    let mut config = ComponentConfig::disabled();
    config.sierra_compiler = sierra_compiler_local_config;
    config.monitoring_endpoint = ActiveComponentExecutionConfig::enabled();
    config
}

fn get_state_sync_component_config(
    state_sync_local_config: ReactiveComponentExecutionConfig,
    class_manager_remote_config: ReactiveComponentExecutionConfig,
) -> ComponentConfig {
    let mut config = ComponentConfig::disabled();
    config.state_sync = state_sync_local_config;
    config.class_manager = class_manager_remote_config;
    config.monitoring_endpoint = ActiveComponentExecutionConfig::enabled();
    config
}

fn get_consensus_manager_component_config(
    batcher_remote_config: ReactiveComponentExecutionConfig,
    class_manager_remote_config: ReactiveComponentExecutionConfig,
    l1_gas_price_provider_remote_config: ReactiveComponentExecutionConfig,
    state_sync_remote_config: ReactiveComponentExecutionConfig,
) -> ComponentConfig {
    let mut config = ComponentConfig::disabled();
    config.consensus_manager = ActiveComponentExecutionConfig::enabled();
    config.batcher = batcher_remote_config;
    config.class_manager = class_manager_remote_config;
    config.l1_gas_price_provider = l1_gas_price_provider_remote_config;
    config.state_sync = state_sync_remote_config;
    config.monitoring_endpoint = ActiveComponentExecutionConfig::enabled();
    config
}

fn get_http_server_component_config(
    gateway_remote_config: ReactiveComponentExecutionConfig,
) -> ComponentConfig {
    let mut config = ComponentConfig::disabled();
    config.http_server = ActiveComponentExecutionConfig::enabled();
    config.gateway = gateway_remote_config;
    config.monitoring_endpoint = ActiveComponentExecutionConfig::enabled();
    config
}

fn get_l1_component_config(
    l1_gas_price_provider_local_config: ReactiveComponentExecutionConfig,
    l1_provider_local_config: ReactiveComponentExecutionConfig,
    state_sync_remote_config: ReactiveComponentExecutionConfig,
) -> ComponentConfig {
    let mut config = ComponentConfig::disabled();
    config.l1_gas_price_provider = l1_gas_price_provider_local_config;
    config.l1_gas_price_scraper = ActiveComponentExecutionConfig::enabled();
    config.l1_provider = l1_provider_local_config;
    config.l1_scraper = ActiveComponentExecutionConfig::enabled();
    config.state_sync = state_sync_remote_config;
    config.monitoring_endpoint = ActiveComponentExecutionConfig::enabled();
    config
}
