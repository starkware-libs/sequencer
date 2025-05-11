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

const BASE_PORT: u16 = 15000; // TODO(Tsabary): arbitrary port, need to resolve.

const BATCHER_STORAGE: usize = 500;
const CLASS_MANAGER_STORAGE: usize = 500;
const STATE_SYNC_STORAGE: usize = 500;

// TODO(Tsabary): define consts and functions whenever relevant.

#[repr(u16)]
#[derive(Clone, Copy, Debug, Display, PartialEq, Eq, Hash, Serialize, AsRefStr, EnumIter)]
#[strum(serialize_all = "snake_case")]
pub enum DistributedNodeServiceName {
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

// Implement conversion from `DistributedNodeServiceName` to `ServiceName`
impl From<DistributedNodeServiceName> for ServiceName {
    fn from(service: DistributedNodeServiceName) -> Self {
        ServiceName::DistributedNode(service)
    }
}

impl GetComponentConfigs for DistributedNodeServiceName {
    fn get_component_configs(
        base_port: Option<u16>,
        environment: &Environment,
    ) -> IndexMap<ServiceName, ComponentConfig> {
        let mut component_config_map = IndexMap::<ServiceName, ComponentConfig>::new();

        // TODO(Tsabary): the following is a temporary solution to differentiate the l1 provider
        // and the l1 gas price provider ports. Need to come up with a better way for that.
        // The offset value has to exceed 3, to avoid conflicting with the remaining services:
        // mempool, sierra compiler, and state sync. The value of 5 was chosen arbitrarily
        // to satisfy the above.
        let base_port_with_offset = Some(base_port.unwrap_or(BASE_PORT) + 5);

        let batcher =
            DistributedNodeServiceName::Batcher.component_config_pair(base_port, environment);
        let class_manager =
            DistributedNodeServiceName::ClassManager.component_config_pair(base_port, environment);
        let gateway =
            DistributedNodeServiceName::Gateway.component_config_pair(base_port, environment);
        let l1_gas_price_provider = DistributedNodeServiceName::L1
            .component_config_pair(base_port_with_offset, environment);
        let l1_provider =
            DistributedNodeServiceName::L1.component_config_pair(base_port, environment);
        let mempool =
            DistributedNodeServiceName::Mempool.component_config_pair(base_port, environment);
        let sierra_compiler = DistributedNodeServiceName::SierraCompiler
            .component_config_pair(base_port, environment);
        let state_sync =
            DistributedNodeServiceName::StateSync.component_config_pair(base_port, environment);

        for inner_service_name in DistributedNodeServiceName::iter() {
            let component_config = match inner_service_name {
                DistributedNodeServiceName::Batcher => get_batcher_component_config(
                    batcher.local(),
                    class_manager.remote(),
                    l1_provider.remote(),
                    mempool.remote(),
                ),
                DistributedNodeServiceName::ClassManager => get_class_manager_component_config(
                    class_manager.local(),
                    sierra_compiler.remote(),
                ),
                DistributedNodeServiceName::ConsensusManager => {
                    get_consensus_manager_component_config(
                        batcher.remote(),
                        class_manager.remote(),
                        l1_gas_price_provider.remote(),
                        state_sync.remote(),
                    )
                }
                DistributedNodeServiceName::HttpServer => {
                    get_http_server_component_config(gateway.remote())
                }
                DistributedNodeServiceName::Gateway => get_gateway_component_config(
                    gateway.local(),
                    class_manager.remote(),
                    mempool.remote(),
                    state_sync.remote(),
                ),
                DistributedNodeServiceName::L1 => get_l1_component_config(
                    l1_gas_price_provider.local(),
                    l1_provider.local(),
                    state_sync.remote(),
                    batcher.remote(),
                ),
                DistributedNodeServiceName::Mempool => get_mempool_component_config(
                    mempool.local(),
                    class_manager.remote(),
                    gateway.remote(),
                ),
                DistributedNodeServiceName::SierraCompiler => {
                    get_sierra_compiler_component_config(sierra_compiler.local())
                }
                DistributedNodeServiceName::StateSync => {
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
impl ServiceNameInner for DistributedNodeServiceName {
    fn create_service(
        &self,
        environment: &Environment,
        external_secret: &Option<ExternalSecret>,
        additional_config_filenames: Vec<String>,
        ingress_params: IngressParams,
    ) -> Service {
        match environment {
            Environment::Testing => match self {
                DistributedNodeServiceName::Batcher => Service::new(
                    Into::<ServiceName>::into(*self),
                    1,
                    Resources::new(Resource::new(1, 2), Resource::new(4, 8)),
                    external_secret.clone(),
                    additional_config_filenames,
                    ingress_params.clone(),
                    environment.clone(),
                ),
                DistributedNodeServiceName::ClassManager => Service::new(
                    Into::<ServiceName>::into(*self),
                    1,
                    Resources::new(Resource::new(1, 2), Resource::new(4, 8)),
                    external_secret.clone(),
                    additional_config_filenames,
                    ingress_params.clone(),
                    environment.clone(),
                ),
                DistributedNodeServiceName::ConsensusManager => Service::new(
                    Into::<ServiceName>::into(*self),
                    1,
                    Resources::new(Resource::new(1, 2), Resource::new(4, 8)),
                    external_secret.clone(),
                    additional_config_filenames,
                    ingress_params.clone(),
                    environment.clone(),
                ),
                DistributedNodeServiceName::HttpServer => Service::new(
                    Into::<ServiceName>::into(*self),
                    1,
                    Resources::new(Resource::new(1, 2), Resource::new(4, 8)),
                    external_secret.clone(),
                    additional_config_filenames,
                    ingress_params.clone(),
                    environment.clone(),
                ),
                DistributedNodeServiceName::Gateway => Service::new(
                    Into::<ServiceName>::into(*self),
                    1,
                    Resources::new(Resource::new(1, 2), Resource::new(4, 8)),
                    external_secret.clone(),
                    additional_config_filenames,
                    ingress_params.clone(),
                    environment.clone(),
                ),
                DistributedNodeServiceName::L1 => Service::new(
                    Into::<ServiceName>::into(*self),
                    1,
                    Resources::new(Resource::new(1, 2), Resource::new(4, 8)),
                    external_secret.clone(),
                    additional_config_filenames,
                    ingress_params.clone(),
                    environment.clone(),
                ),
                DistributedNodeServiceName::Mempool => Service::new(
                    Into::<ServiceName>::into(*self),
                    1,
                    Resources::new(Resource::new(1, 2), Resource::new(4, 8)),
                    external_secret.clone(),
                    additional_config_filenames,
                    ingress_params.clone(),
                    environment.clone(),
                ),
                DistributedNodeServiceName::SierraCompiler => Service::new(
                    Into::<ServiceName>::into(*self),
                    1,
                    Resources::new(Resource::new(1, 2), Resource::new(4, 8)),
                    external_secret.clone(),
                    additional_config_filenames,
                    ingress_params.clone(),
                    environment.clone(),
                ),
                DistributedNodeServiceName::StateSync => Service::new(
                    Into::<ServiceName>::into(*self),
                    1,
                    Resources::new(Resource::new(1, 2), Resource::new(4, 8)),
                    external_secret.clone(),
                    additional_config_filenames,
                    ingress_params.clone(),
                    environment.clone(),
                ),
            },
            Environment::SepoliaIntegration => match self {
                DistributedNodeServiceName::Batcher => Service::new(
                    Into::<ServiceName>::into(*self),
                    1,
                    Resources::new(Resource::new(1, 2), Resource::new(4, 8)),
                    external_secret.clone(),
                    additional_config_filenames,
                    ingress_params.clone(),
                    environment.clone(),
                ),
                DistributedNodeServiceName::ClassManager => Service::new(
                    Into::<ServiceName>::into(*self),
                    1,
                    Resources::new(Resource::new(1, 2), Resource::new(4, 8)),
                    external_secret.clone(),
                    additional_config_filenames,
                    ingress_params.clone(),
                    environment.clone(),
                ),
                DistributedNodeServiceName::ConsensusManager => Service::new(
                    Into::<ServiceName>::into(*self),
                    1,
                    Resources::new(Resource::new(1, 2), Resource::new(4, 8)),
                    external_secret.clone(),
                    additional_config_filenames,
                    ingress_params.clone(),
                    environment.clone(),
                ),
                DistributedNodeServiceName::HttpServer => Service::new(
                    Into::<ServiceName>::into(*self),
                    1,
                    Resources::new(Resource::new(1, 2), Resource::new(4, 8)),
                    external_secret.clone(),
                    additional_config_filenames,
                    ingress_params.clone(),
                    environment.clone(),
                ),
                DistributedNodeServiceName::Gateway => Service::new(
                    Into::<ServiceName>::into(*self),
                    1,
                    Resources::new(Resource::new(1, 2), Resource::new(4, 8)),
                    external_secret.clone(),
                    additional_config_filenames,
                    ingress_params.clone(),
                    environment.clone(),
                ),
                DistributedNodeServiceName::L1 => Service::new(
                    Into::<ServiceName>::into(*self),
                    1,
                    Resources::new(Resource::new(1, 2), Resource::new(4, 8)),
                    external_secret.clone(),
                    additional_config_filenames,
                    ingress_params.clone(),
                    environment.clone(),
                ),
                DistributedNodeServiceName::Mempool => Service::new(
                    Into::<ServiceName>::into(*self),
                    1,
                    Resources::new(Resource::new(1, 2), Resource::new(4, 8)),
                    external_secret.clone(),
                    additional_config_filenames,
                    ingress_params.clone(),
                    environment.clone(),
                ),
                DistributedNodeServiceName::SierraCompiler => Service::new(
                    Into::<ServiceName>::into(*self),
                    1,
                    Resources::new(Resource::new(1, 2), Resource::new(4, 8)),
                    external_secret.clone(),
                    additional_config_filenames,
                    ingress_params.clone(),
                    environment.clone(),
                ),
                DistributedNodeServiceName::StateSync => Service::new(
                    Into::<ServiceName>::into(*self),
                    1,
                    Resources::new(Resource::new(1, 2), Resource::new(4, 8)),
                    external_secret.clone(),
                    additional_config_filenames,
                    ingress_params.clone(),
                    environment.clone(),
                ),
            },
            _ => unimplemented!(),
        }
    }

    // TODO(Tsabary/Idan): set the correct controller type for each service.
    fn get_controller(&self) -> Controller {
        match self {
            DistributedNodeServiceName::Batcher => Controller::StatefulSet,
            DistributedNodeServiceName::ClassManager => Controller::StatefulSet,
            DistributedNodeServiceName::ConsensusManager => Controller::StatefulSet,
            DistributedNodeServiceName::HttpServer => Controller::StatefulSet,
            DistributedNodeServiceName::Gateway => Controller::StatefulSet,
            DistributedNodeServiceName::L1 => Controller::StatefulSet,
            DistributedNodeServiceName::Mempool => Controller::StatefulSet,
            DistributedNodeServiceName::SierraCompiler => Controller::StatefulSet,
            DistributedNodeServiceName::StateSync => Controller::StatefulSet,
        }
    }

    fn get_autoscale(&self) -> bool {
        match self {
            DistributedNodeServiceName::Batcher => false,
            DistributedNodeServiceName::ClassManager => false,
            DistributedNodeServiceName::ConsensusManager => false,
            DistributedNodeServiceName::HttpServer => false,
            DistributedNodeServiceName::Gateway => true,
            DistributedNodeServiceName::L1 => false,
            DistributedNodeServiceName::Mempool => false,
            DistributedNodeServiceName::SierraCompiler => true,
            DistributedNodeServiceName::StateSync => false,
        }
    }

    fn get_toleration(&self, environment: &Environment) -> Option<Toleration> {
        match environment {
            Environment::Testing => None,
            Environment::SepoliaIntegration
            | Environment::TestingEnvTwo
            | Environment::TestingEnvThree => match self {
                DistributedNodeServiceName::Batcher => Some(Toleration::ApolloCoreService),
                DistributedNodeServiceName::ClassManager => Some(Toleration::ApolloGeneralService),
                DistributedNodeServiceName::ConsensusManager => {
                    Some(Toleration::ApolloGeneralService)
                }
                DistributedNodeServiceName::HttpServer => Some(Toleration::ApolloGeneralService),
                DistributedNodeServiceName::Gateway => Some(Toleration::ApolloGeneralService),
                DistributedNodeServiceName::L1 => Some(Toleration::ApolloGeneralService),
                DistributedNodeServiceName::Mempool => Some(Toleration::ApolloGeneralService),
                DistributedNodeServiceName::SierraCompiler => {
                    Some(Toleration::ApolloGeneralService)
                }
                DistributedNodeServiceName::StateSync => Some(Toleration::ApolloGeneralService),
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
            DistributedNodeServiceName::Batcher => None,
            DistributedNodeServiceName::ClassManager => None,
            DistributedNodeServiceName::ConsensusManager => None,
            DistributedNodeServiceName::HttpServer => {
                let internal = match environment {
                    Environment::Testing => true,
                    Environment::SepoliaIntegration
                    | Environment::TestingEnvTwo
                    | Environment::TestingEnvThree => false,
                    _ => unimplemented!(),
                };
                get_ingress(ingress_params, internal)
            }
            DistributedNodeServiceName::Gateway => None,
            DistributedNodeServiceName::L1 => None,
            DistributedNodeServiceName::Mempool => None,
            DistributedNodeServiceName::SierraCompiler => None,
            DistributedNodeServiceName::StateSync => None,
        }
    }

    fn get_storage(&self, environment: &Environment) -> Option<usize> {
        match environment {
            Environment::Testing => None,
            Environment::SepoliaIntegration
            | Environment::TestingEnvTwo
            | Environment::TestingEnvThree => match self {
                DistributedNodeServiceName::Batcher => Some(BATCHER_STORAGE),
                DistributedNodeServiceName::ClassManager => Some(CLASS_MANAGER_STORAGE),
                DistributedNodeServiceName::ConsensusManager => None,
                DistributedNodeServiceName::HttpServer => None,
                DistributedNodeServiceName::Gateway => None,
                DistributedNodeServiceName::L1 => None,
                DistributedNodeServiceName::Mempool => None,
                DistributedNodeServiceName::SierraCompiler => None,
                DistributedNodeServiceName::StateSync => Some(STATE_SYNC_STORAGE),
            },
            _ => unimplemented!(),
        }
    }
}

impl DistributedNodeServiceName {
    // TODO(Tsabary): there's code duplication here that needs to be removed, especially with
    // respect of the hybrid node.

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
    ) -> DistributedNodeServiceConfigPair {
        DistributedNodeServiceConfigPair {
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

/// Component config bundling for services of a distributed node: a config to run a component
/// locally while being accessible to other services, and a suitable config enabling such services
/// the access.
struct DistributedNodeServiceConfigPair {
    local: ReactiveComponentExecutionConfig,
    remote: ReactiveComponentExecutionConfig,
}

impl DistributedNodeServiceConfigPair {
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
    batcher_remote_config: ReactiveComponentExecutionConfig,
) -> ComponentConfig {
    let mut config = ComponentConfig::disabled();

    config.l1_gas_price_provider = l1_gas_price_provider_local_config;
    config.l1_gas_price_scraper = ActiveComponentExecutionConfig::enabled();
    config.l1_provider = l1_provider_local_config;
    config.l1_scraper = ActiveComponentExecutionConfig::enabled();
    config.state_sync = state_sync_remote_config;
    config.monitoring_endpoint = ActiveComponentExecutionConfig::enabled();
    config.batcher = batcher_remote_config;
    config
}
