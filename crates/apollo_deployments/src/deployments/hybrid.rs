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

use crate::config_override::InstanceConfigOverride;
use crate::deployment::{DeploymentType, P2PCommunicationType};
use crate::deployment_definitions::Environment;
use crate::deployments::IDLE_CONNECTIONS_FOR_AUTOSCALED_SERVICES;
use crate::k8s::{
    get_environment_ingress_internal,
    get_ingress,
    Controller,
    Ingress,
    IngressParams,
    Resource,
    Resources,
    Toleration,
};
use crate::service::{GetComponentConfigs, ServiceName, ServiceNameInner};
use crate::utils::{determine_port_numbers, get_secret_key, get_validator_id};

pub const HYBRID_NODE_REQUIRED_PORTS_NUM: usize = 9;

const BASE_PORT: u16 = 55000; // TODO(Tsabary): arbitrary port, need to resolve.
const CORE_STORAGE: usize = 1000;
const MAX_NODE_ID: usize = 9; // Currently supporting up to 9 nodes, to avoid more complicated string manipulations.

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
    fn get_component_configs(ports: Option<Vec<u16>>) -> IndexMap<ServiceName, ComponentConfig> {
        let mut component_config_map = IndexMap::<ServiceName, ComponentConfig>::new();

        let ports = determine_port_numbers(ports, HYBRID_NODE_REQUIRED_PORTS_NUM, BASE_PORT);

        let batcher = HybridNodeServiceName::Core.component_config_pair(ports[0]);
        let class_manager = HybridNodeServiceName::Core.component_config_pair(ports[1]);
        let gateway = HybridNodeServiceName::Gateway.component_config_pair(ports[2]);
        let l1_gas_price_provider = HybridNodeServiceName::Core.component_config_pair(ports[3]);
        let l1_provider = HybridNodeServiceName::Core.component_config_pair(ports[4]);
        let l1_endpoint_monitor = HybridNodeServiceName::Core.component_config_pair(ports[5]);
        let mempool = HybridNodeServiceName::Mempool.component_config_pair(ports[6]);
        let sierra_compiler = HybridNodeServiceName::SierraCompiler.component_config_pair(ports[7]);
        let state_sync = HybridNodeServiceName::Core.component_config_pair(ports[8]);

        for inner_service_name in HybridNodeServiceName::iter() {
            let component_config = match inner_service_name {
                HybridNodeServiceName::Core => get_core_component_config(
                    batcher.local(),
                    class_manager.local(),
                    l1_gas_price_provider.local(),
                    l1_provider.local(),
                    l1_endpoint_monitor.local(),
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
                HybridNodeServiceName::Mempool => Some(Toleration::ApolloCoreService),
                HybridNodeServiceName::SierraCompiler => Some(Toleration::ApolloGeneralService),
            },
            Environment::StressTest => match self {
                HybridNodeServiceName::Core => Some(Toleration::ApolloCoreServiceC2D56),
                HybridNodeServiceName::HttpServer => Some(Toleration::ApolloGeneralService),
                HybridNodeServiceName::Gateway => Some(Toleration::ApolloGeneralService),
                HybridNodeServiceName::Mempool => Some(Toleration::ApolloCoreService),
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

    fn has_p2p_interface(&self) -> bool {
        match self {
            HybridNodeServiceName::Core | HybridNodeServiceName::Mempool => true,
            HybridNodeServiceName::HttpServer
            | HybridNodeServiceName::Gateway
            | HybridNodeServiceName::SierraCompiler => false,
        }
    }

    fn get_storage(&self, environment: &Environment) -> Option<usize> {
        match environment {
            Environment::Testing => None,
            Environment::SepoliaIntegration
            | Environment::TestingEnvTwo
            | Environment::TestingEnvThree
            | Environment::StressTest => match self {
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
            Environment::StressTest => match self {
                HybridNodeServiceName::Core => {
                    Resources::new(Resource::new(50, 200), Resource::new(50, 220))
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
            | Environment::TestingEnvThree
            | Environment::StressTest => match self {
                HybridNodeServiceName::Core => 1,
                HybridNodeServiceName::HttpServer => 1,
                HybridNodeServiceName::Gateway => 2,
                HybridNodeServiceName::Mempool => 1,
                HybridNodeServiceName::SierraCompiler => 2,
            },
            _ => unimplemented!(),
        }
    }

    fn get_anti_affinity(&self, environment: &Environment) -> bool {
        match environment {
            Environment::Testing => false,
            Environment::SepoliaIntegration
            | Environment::TestingEnvTwo
            | Environment::TestingEnvThree
            | Environment::StressTest => match self {
                HybridNodeServiceName::Core => true,
                HybridNodeServiceName::HttpServer => false,
                HybridNodeServiceName::Gateway => false,
                HybridNodeServiceName::Mempool => false,
                HybridNodeServiceName::SierraCompiler => false,
            },
            _ => unimplemented!(),
        }
    }
}

impl HybridNodeServiceName {
    /// Returns a component execution config for a component that runs locally, and accepts inbound
    /// connections from remote components.
    fn component_config_for_local_service(&self, port: u16) -> ReactiveComponentExecutionConfig {
        ReactiveComponentExecutionConfig::local_with_remote_enabled(
            self.k8s_service_name(),
            IpAddr::from(Ipv4Addr::UNSPECIFIED),
            port,
        )
    }

    /// Returns a component execution config for a component that is accessed remotely.
    fn component_config_for_remote_service(&self, port: u16) -> ReactiveComponentExecutionConfig {
        let mut base = ReactiveComponentExecutionConfig::remote(
            self.k8s_service_name(),
            IpAddr::from(Ipv4Addr::UNSPECIFIED),
            port,
        );
        match self {
            HybridNodeServiceName::Gateway | HybridNodeServiceName::SierraCompiler => {
                base.remote_client_config.idle_connections =
                    IDLE_CONNECTIONS_FOR_AUTOSCALED_SERVICES;
            }
            HybridNodeServiceName::Core
            | HybridNodeServiceName::HttpServer
            | HybridNodeServiceName::Mempool => {}
        };
        base
    }

    fn component_config_pair(&self, port: u16) -> HybridNodeServiceConfigPair {
        HybridNodeServiceConfigPair {
            local: self.component_config_for_local_service(port),
            remote: self.component_config_for_remote_service(port),
        }
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

#[allow(clippy::too_many_arguments)]
fn get_core_component_config(
    batcher_local_config: ReactiveComponentExecutionConfig,
    class_manager_local_config: ReactiveComponentExecutionConfig,
    l1_gas_price_provider_local_config: ReactiveComponentExecutionConfig,
    l1_provider_local_config: ReactiveComponentExecutionConfig,
    l1_endpoint_monitor_local_config: ReactiveComponentExecutionConfig,
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
    config.l1_endpoint_monitor = l1_endpoint_monitor_local_config;
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

pub(crate) fn create_hybrid_instance_config_override(
    id: usize,
    namespace: &'static str,
    deployment_type: DeploymentType,
    p2p_communication_type: P2PCommunicationType,
    domain: &str,
) -> InstanceConfigOverride {
    assert!(id < MAX_NODE_ID, "Node id {} exceeds the number of nodes {}", id, MAX_NODE_ID);

    // TODO(Tsabary): these should be derived from the hybrid deployment module, and used
    // consistently throughout the code.

    // This node address uses that the first node secret key is
    // "0x0101010101010101010101010101010101010101010101010101010101010101".
    // TODO(Tsabary): test to enforce the above assumption.
    const FIRST_NODE_ADDRESS: &str = "12D3KooWK99VoVxNE7XzyBwXEzW7xhK7Gpv85r9F3V3fyKSUKPH5";
    const CORE_SERVICE_PORT: u16 = 53080;
    const MEMPOOL_SERVICE_PORT: u16 = 53200;

    if id == 0 {
        InstanceConfigOverride::new(
            None,
            get_secret_key(id),
            None,
            get_secret_key(id),
            get_validator_id(id, deployment_type),
        )
    } else {
        InstanceConfigOverride::new(
            Some(p2p_communication_type.get_p2p_address(
                &HybridNodeServiceName::Core.k8s_service_name(),
                namespace,
                domain,
                CORE_SERVICE_PORT,
                FIRST_NODE_ADDRESS,
            )),
            get_secret_key(id),
            Some(p2p_communication_type.get_p2p_address(
                &HybridNodeServiceName::Mempool.k8s_service_name(),
                namespace,
                domain,
                MEMPOOL_SERVICE_PORT,
                FIRST_NODE_ADDRESS,
            )),
            get_secret_key(id),
            get_validator_id(id, deployment_type),
        )
    }
}
