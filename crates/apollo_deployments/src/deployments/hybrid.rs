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

use crate::deployment_definitions::{Environment, EnvironmentComponentConfigModifications};
use crate::service::{
    get_environment_ingress_internal,
    get_ingress,
    Controller,
    ExternalSecret,
    GetComponentConfigs,
    Ingress,
    IngressParams,
    Resource,
    Resources,
    Service,
    ServiceName,
    ServiceNameInner,
    Toleration,
};

const BASE_PORT: u16 = 55000; // TODO(Tsabary): arbitrary port, need to resolve.

const CORE_STORAGE: usize = 1000;

#[repr(u16)]
#[derive(Clone, Copy, Debug, Display, PartialEq, Eq, Hash, Serialize, AsRefStr, EnumIter)]
#[strum(serialize_all = "snake_case")]
pub enum HybridNodeServiceName {
    Core, /* Comprises the batcher, class manager, consensus manager, l1 components, and state
           * sync. */
    HttpServer,
    Gateway,
    Mempool,
    SierraCompiler,
}

// Implement conversion from `HybridNodeServiceName` to `ServiceName`
impl From<HybridNodeServiceName> for ServiceName {
    fn from(service: HybridNodeServiceName) -> Self {
        ServiceName::HybridNode(service)
    }
}

impl GetComponentConfigs for HybridNodeServiceName {
    fn get_component_configs(
        base_port: Option<u16>,
        environment: &Environment,
    ) -> IndexMap<ServiceName, ComponentConfig> {
        // TODO(Tsabary): change this function to take a slice of port numbers at the exact expected
        // length.
        let mut component_config_map = IndexMap::<ServiceName, ComponentConfig>::new();

        let base_port_with_offset = base_port.unwrap_or(BASE_PORT);

        let batcher = HybridNodeServiceName::Core
            .component_config_pair(Some(base_port_with_offset), environment);
        let class_manager = HybridNodeServiceName::Core
            .component_config_pair(Some(base_port_with_offset + 1), environment);
        let gateway = HybridNodeServiceName::Gateway
            .component_config_pair(Some(base_port_with_offset + 2), environment);
        let l1_gas_price_provider = HybridNodeServiceName::Core
            .component_config_pair(Some(base_port_with_offset + 3), environment);
        let l1_provider = HybridNodeServiceName::Core
            .component_config_pair(Some(base_port_with_offset + 4), environment);
        let mempool = HybridNodeServiceName::Mempool
            .component_config_pair(Some(base_port_with_offset + 5), environment);
        let sierra_compiler = HybridNodeServiceName::SierraCompiler
            .component_config_pair(Some(base_port_with_offset + 6), environment);
        let state_sync = HybridNodeServiceName::Core
            .component_config_pair(Some(base_port_with_offset + 7), environment);

        for inner_service_name in HybridNodeServiceName::iter() {
            let component_config = match inner_service_name {
                HybridNodeServiceName::Core => get_core_component_config(
                    batcher.local(),
                    class_manager.local(),
                    l1_gas_price_provider.local(),
                    l1_provider.local(),
                    state_sync.local(),
                    mempool.remote(),
                    sierra_compiler.remote(),
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
                HybridNodeServiceName::Mempool => get_mempool_component_config(
                    mempool.local(),
                    class_manager.remote(),
                    gateway.remote(),
                ),
                HybridNodeServiceName::SierraCompiler => {
                    get_sierra_compiler_component_config(sierra_compiler.local())
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
    fn create_service(
        &self,
        environment: &Environment,
        external_secret: &Option<ExternalSecret>,
        additional_config_filenames: Vec<String>,
        ingress_params: IngressParams,
    ) -> Service {
        match environment {
            Environment::Testing => match self {
                HybridNodeServiceName::Core => Service::new(
                    Into::<ServiceName>::into(*self),
                    external_secret.clone(),
                    additional_config_filenames,
                    ingress_params.clone(),
                    environment.clone(),
                ),
                HybridNodeServiceName::HttpServer => Service::new(
                    Into::<ServiceName>::into(*self),
                    external_secret.clone(),
                    additional_config_filenames,
                    ingress_params.clone(),
                    environment.clone(),
                ),
                HybridNodeServiceName::Gateway => Service::new(
                    Into::<ServiceName>::into(*self),
                    external_secret.clone(),
                    additional_config_filenames,
                    ingress_params.clone(),
                    environment.clone(),
                ),
                HybridNodeServiceName::Mempool => Service::new(
                    Into::<ServiceName>::into(*self),
                    external_secret.clone(),
                    additional_config_filenames,
                    ingress_params.clone(),
                    environment.clone(),
                ),
                HybridNodeServiceName::SierraCompiler => Service::new(
                    Into::<ServiceName>::into(*self),
                    external_secret.clone(),
                    additional_config_filenames,
                    ingress_params.clone(),
                    environment.clone(),
                ),
            },
            Environment::SepoliaIntegration
            | Environment::TestingEnvTwo
            | Environment::TestingEnvThree => match self {
                HybridNodeServiceName::Core => Service::new(
                    Into::<ServiceName>::into(*self),
                    external_secret.clone(),
                    additional_config_filenames,
                    ingress_params.clone(),
                    environment.clone(),
                ),
                HybridNodeServiceName::HttpServer => Service::new(
                    Into::<ServiceName>::into(*self),
                    external_secret.clone(),
                    additional_config_filenames,
                    ingress_params.clone(),
                    environment.clone(),
                ),
                HybridNodeServiceName::Gateway => Service::new(
                    Into::<ServiceName>::into(*self),
                    external_secret.clone(),
                    additional_config_filenames,
                    ingress_params.clone(),
                    environment.clone(),
                ),
                HybridNodeServiceName::Mempool => Service::new(
                    Into::<ServiceName>::into(*self),
                    external_secret.clone(),
                    additional_config_filenames,
                    ingress_params.clone(),
                    environment.clone(),
                ),
                HybridNodeServiceName::SierraCompiler => Service::new(
                    Into::<ServiceName>::into(*self),
                    external_secret.clone(),
                    additional_config_filenames,
                    ingress_params.clone(),
                    environment.clone(),
                ),
            },
            _ => unimplemented!(),
        }
    }

    fn get_controller(&self) -> Controller {
        match self {
            HybridNodeServiceName::Core => Controller::StatefulSet,
            HybridNodeServiceName::HttpServer => Controller::Deployment,
            HybridNodeServiceName::Gateway => Controller::Deployment,
            HybridNodeServiceName::Mempool => Controller::Deployment,
            HybridNodeServiceName::SierraCompiler => Controller::Deployment,
        }
    }

    fn get_autoscale(&self) -> bool {
        match self {
            HybridNodeServiceName::Core => false,
            HybridNodeServiceName::HttpServer => false,
            HybridNodeServiceName::Gateway => true,
            HybridNodeServiceName::Mempool => false,
            HybridNodeServiceName::SierraCompiler => true,
        }
    }

    fn get_toleration(&self, environment: &Environment) -> Option<Toleration> {
        match environment {
            Environment::Testing => None,
            Environment::SepoliaIntegration
            | Environment::TestingEnvTwo
            | Environment::TestingEnvThree => match self {
                HybridNodeServiceName::Core => Some(Toleration::ApolloCoreService),
                HybridNodeServiceName::HttpServer => Some(Toleration::ApolloGeneralService),
                HybridNodeServiceName::Gateway => Some(Toleration::ApolloGeneralService),
                HybridNodeServiceName::Mempool => Some(Toleration::ApolloGeneralService),
                HybridNodeServiceName::SierraCompiler => Some(Toleration::ApolloGeneralService),
            },
            _ => unimplemented!(),
        }
    }

    fn get_ingress(
        &self,
        environment: &Environment,
        ingress_params: IngressParams,
    ) -> Option<Ingress> {
        match self {
            HybridNodeServiceName::Core => None,
            HybridNodeServiceName::HttpServer => {
                get_ingress(ingress_params, get_environment_ingress_internal(environment))
            }
            HybridNodeServiceName::Gateway => None,
            HybridNodeServiceName::Mempool => None,
            HybridNodeServiceName::SierraCompiler => None,
        }
    }

    fn get_storage(&self, environment: &Environment) -> Option<usize> {
        match environment {
            Environment::Testing => None,
            Environment::SepoliaIntegration
            | Environment::TestingEnvTwo
            | Environment::TestingEnvThree => match self {
                HybridNodeServiceName::Core => Some(CORE_STORAGE),
                HybridNodeServiceName::HttpServer => None,
                HybridNodeServiceName::Gateway => None,
                HybridNodeServiceName::Mempool => None,
                HybridNodeServiceName::SierraCompiler => None,
            },
            _ => unimplemented!(),
        }
    }

    fn get_resources(&self, environment: &Environment) -> Resources {
        match environment {
            Environment::Testing => Resources::new(Resource::new(1, 2), Resource::new(4, 8)),
            Environment::SepoliaIntegration
            | Environment::TestingEnvTwo
            | Environment::TestingEnvThree => match self {
                HybridNodeServiceName::Core => {
                    Resources::new(Resource::new(2, 4), Resource::new(7, 14))
                }
                HybridNodeServiceName::HttpServer => {
                    Resources::new(Resource::new(1, 2), Resource::new(4, 8))
                }
                HybridNodeServiceName::Gateway => {
                    Resources::new(Resource::new(1, 2), Resource::new(2, 4))
                }
                HybridNodeServiceName::Mempool => {
                    Resources::new(Resource::new(1, 2), Resource::new(2, 4))
                }
                HybridNodeServiceName::SierraCompiler => {
                    Resources::new(Resource::new(1, 2), Resource::new(2, 4))
                }
            },
            _ => unimplemented!(),
        }
    }

    fn get_replicas(&self, environment: &Environment) -> usize {
        match environment {
            Environment::Testing => 1,
            Environment::SepoliaIntegration
            | Environment::TestingEnvTwo
            | Environment::TestingEnvThree => match self {
                HybridNodeServiceName::Core => 1,
                HybridNodeServiceName::HttpServer => 1,
                HybridNodeServiceName::Gateway => 2,
                HybridNodeServiceName::Mempool => 1,
                HybridNodeServiceName::SierraCompiler => 2,
            },
            _ => unimplemented!(),
        }
    }
}

impl HybridNodeServiceName {
    /// Returns a component execution config for a component that runs locally, and accepts inbound
    /// connections from remote components.
    pub fn component_config_for_local_service(
        &self,
        base_port: Option<u16>,
        environment: &Environment,
    ) -> ReactiveComponentExecutionConfig {
        let mut base = ReactiveComponentExecutionConfig::local_with_remote_enabled(
            self.url(),
            self.ip(),
            self.port(base_port),
        );
        let EnvironmentComponentConfigModifications {
            local_server_config,
            max_concurrency,
            remote_client_config: _,
        } = environment.get_component_config_modifications();
        base.local_server_config = local_server_config;
        base.max_concurrency = max_concurrency;
        base
    }

    /// Returns a component execution config for a component that is accessed remotely.
    pub fn component_config_for_remote_service(
        &self,
        base_port: Option<u16>,
        environment: &Environment,
    ) -> ReactiveComponentExecutionConfig {
        let mut base =
            ReactiveComponentExecutionConfig::remote(self.url(), self.ip(), self.port(base_port));
        let EnvironmentComponentConfigModifications {
            local_server_config: _,
            max_concurrency,
            remote_client_config,
        } = environment.get_component_config_modifications();
        base.remote_client_config = remote_client_config;
        base.max_concurrency = max_concurrency;
        base
    }

    fn component_config_pair(
        &self,
        base_port: Option<u16>,
        environment: &Environment,
    ) -> HybridNodeServiceConfigPair {
        HybridNodeServiceConfigPair {
            local: self.component_config_for_local_service(base_port, environment),
            remote: self.component_config_for_remote_service(base_port, environment),
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
        base_port.unwrap_or(BASE_PORT)
    }

    /// Listening address per service.
    fn ip(&self) -> IpAddr {
        IpAddr::from(Ipv4Addr::UNSPECIFIED)
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

fn get_core_component_config(
    batcher_local_config: ReactiveComponentExecutionConfig,
    class_manager_local_config: ReactiveComponentExecutionConfig,
    l1_gas_price_provider_local_config: ReactiveComponentExecutionConfig,
    l1_provider_local_config: ReactiveComponentExecutionConfig,
    state_sync_local_config: ReactiveComponentExecutionConfig,
    mempool_remote_config: ReactiveComponentExecutionConfig,
    sierra_compiler_remote_config: ReactiveComponentExecutionConfig,
) -> ComponentConfig {
    let mut config = ComponentConfig::disabled();
    config.batcher = batcher_local_config;
    config.class_manager = class_manager_local_config;
    config.consensus_manager = ActiveComponentExecutionConfig::enabled();
    config.l1_gas_price_provider = l1_gas_price_provider_local_config;
    config.l1_gas_price_scraper = ActiveComponentExecutionConfig::enabled();
    config.l1_provider = l1_provider_local_config;
    config.l1_scraper = ActiveComponentExecutionConfig::enabled();
    config.sierra_compiler = sierra_compiler_remote_config;
    config.state_sync = state_sync_local_config;
    config.mempool = mempool_remote_config;
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

fn get_http_server_component_config(
    gateway_remote_config: ReactiveComponentExecutionConfig,
) -> ComponentConfig {
    let mut config = ComponentConfig::disabled();
    config.http_server = ActiveComponentExecutionConfig::enabled();
    config.gateway = gateway_remote_config;
    config.monitoring_endpoint = ActiveComponentExecutionConfig::enabled();
    config
}
