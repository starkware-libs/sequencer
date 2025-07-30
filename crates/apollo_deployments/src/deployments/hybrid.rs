use std::collections::BTreeSet;
use std::net::{IpAddr, Ipv4Addr};

use apollo_infra_utils::path::resolve_project_relative_path;
use apollo_infra_utils::template::Template;
use apollo_node::config::component_config::ComponentConfig;
use apollo_node::config::component_execution_config::{
    ActiveComponentExecutionConfig,
    ReactiveComponentExecutionConfig,
};
use indexmap::IndexMap;
use serde::Serialize;
use strum::{Display, IntoEnumIterator};
use strum_macros::{AsRefStr, EnumIter};

use crate::addresses::{get_p2p_address, get_peer_id};
use crate::config_override::{
    ConfigOverride,
    DeploymentConfigOverride,
    InstanceConfigOverride,
    NetworkConfigOverride,
};
use crate::deployment::{build_service_namespace_domain_address, Deployment, P2PCommunicationType};
<<<<<<< HEAD
use crate::deployment_definitions::{
    CloudK8sEnvironment,
    ComponentConfigInService,
    DeploymentInputs,
    Environment,
    ServicePort,
};
||||||| 923579022
use crate::deployment_definitions::{CloudK8sEnvironment, Environment, ServicePort};
=======
use crate::deployment_definitions::{
    BusinessLogicServicePort,
    CloudK8sEnvironment,
    Environment,
    InfraServicePort,
    ServicePort,
};
>>>>>>> origin/main
use crate::deployments::IDLE_CONNECTIONS_FOR_AUTOSCALED_SERVICES;
use crate::k8s::{
    get_environment_ingress_internal,
    get_ingress,
    Controller,
    ExternalSecret,
    Ingress,
    IngressParams,
    K8sServiceConfigParams,
    Resource,
    Resources,
    Toleration,
};
use crate::service::{GetComponentConfigs, NodeService, NodeType, ServiceNameInner};
use crate::utils::{determine_port_numbers, get_validator_id};

pub const HYBRID_NODE_REQUIRED_PORTS_NUM: usize = 9;
pub(crate) const INSTANCE_NAME_FORMAT: &str = "hybrid_{}";

const BASE_PORT: u16 = 55000; // TODO(Tsabary): arbitrary port, need to resolve.
const CORE_STORAGE: usize = 1000;
const TEST_CORE_STORAGE: usize = 1;
const MAX_NODE_ID: usize = 9; // Currently supporting up to 9 nodes, to avoid more complicated string manipulations.

#[derive(Clone, Copy, Debug, Display, PartialEq, Eq, Hash, Serialize, AsRefStr, EnumIter)]
#[strum(serialize_all = "snake_case")]
pub enum HybridNodeServiceName {
    Core, // Comprises the batcher, class manager, consensus manager, and state sync.
    HttpServer,
    Gateway,
    L1, // Comprises the various l1 components.
    Mempool,
    SierraCompiler,
}

// Implement conversion from `HybridNodeServiceName` to `NodeService`
impl From<HybridNodeServiceName> for NodeService {
    fn from(service: HybridNodeServiceName) -> Self {
        NodeService::Hybrid(service)
    }
}

impl GetComponentConfigs for HybridNodeServiceName {
    fn get_component_configs(ports: Option<Vec<u16>>) -> IndexMap<NodeService, ComponentConfig> {
        let mut component_config_map = IndexMap::<NodeService, ComponentConfig>::new();

        let ports = determine_port_numbers(ports, HYBRID_NODE_REQUIRED_PORTS_NUM, BASE_PORT);

        let batcher = HybridNodeServiceName::Core.component_config_pair(ports[0]);
        let class_manager = HybridNodeServiceName::Core.component_config_pair(ports[1]);
        let gateway = HybridNodeServiceName::Gateway.component_config_pair(ports[2]);
        let l1_gas_price_provider = HybridNodeServiceName::L1.component_config_pair(ports[3]);
        let l1_provider = HybridNodeServiceName::L1.component_config_pair(ports[4]);
        let mempool = HybridNodeServiceName::Mempool.component_config_pair(ports[5]);
        let sierra_compiler = HybridNodeServiceName::SierraCompiler.component_config_pair(ports[6]);
        let state_sync = HybridNodeServiceName::Core.component_config_pair(ports[7]);
        let signature_manager = HybridNodeServiceName::Core.component_config_pair(ports[8]);

        for inner_service_name in HybridNodeServiceName::iter() {
            let component_config = match inner_service_name {
                HybridNodeServiceName::Core => get_core_component_config(
                    batcher.local(),
                    class_manager.local(),
                    l1_gas_price_provider.remote(),
                    l1_provider.remote(),
                    state_sync.local(),
                    mempool.remote(),
                    sierra_compiler.remote(),
                    signature_manager.remote(),
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
                    batcher.remote(),
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
            let node_service = inner_service_name.into();
            component_config_map.insert(node_service, component_config);
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
            HybridNodeServiceName::L1 => Controller::Deployment,
            HybridNodeServiceName::Mempool => Controller::Deployment,
            HybridNodeServiceName::SierraCompiler => Controller::Deployment,
        }
    }

    fn get_autoscale(&self) -> bool {
        match self {
            HybridNodeServiceName::Core => false,
            HybridNodeServiceName::HttpServer => false,
            HybridNodeServiceName::Gateway => true,
            HybridNodeServiceName::L1 => false,
            HybridNodeServiceName::Mempool => false,
            HybridNodeServiceName::SierraCompiler => true,
        }
    }

    fn get_toleration(&self, environment: &Environment) -> Option<Toleration> {
        match environment {
            Environment::CloudK8s(cloud_env) => match cloud_env {
                CloudK8sEnvironment::SepoliaIntegration | CloudK8sEnvironment::UpgradeTest => {
                    match self {
                        HybridNodeServiceName::Core | HybridNodeServiceName::Mempool => {
                            Some(Toleration::ApolloCoreService)
                        }
                        HybridNodeServiceName::HttpServer
                        | HybridNodeServiceName::Gateway
                        | HybridNodeServiceName::L1
                        | HybridNodeServiceName::SierraCompiler => {
                            Some(Toleration::ApolloGeneralService)
                        }
                    }
                }
                CloudK8sEnvironment::Mainnet
                | CloudK8sEnvironment::SepoliaTestnet
                | CloudK8sEnvironment::StressTest => match self {
                    HybridNodeServiceName::Core => Some(Toleration::ApolloCoreServiceC2D56),
                    HybridNodeServiceName::HttpServer
                    | HybridNodeServiceName::Gateway
                    | HybridNodeServiceName::L1
                    | HybridNodeServiceName::SierraCompiler => {
                        Some(Toleration::ApolloGeneralService)
                    }
                    HybridNodeServiceName::Mempool => Some(Toleration::ApolloCoreService),
                },
                CloudK8sEnvironment::Potc2 => match self {
                    HybridNodeServiceName::Core => Some(Toleration::Batcher864),
                    HybridNodeServiceName::HttpServer
                    | HybridNodeServiceName::Gateway
                    | HybridNodeServiceName::L1
                    | HybridNodeServiceName::SierraCompiler => {
                        Some(Toleration::ApolloGeneralService)
                    }
                    HybridNodeServiceName::Mempool => Some(Toleration::ApolloCoreService),
                },
            },
            Environment::LocalK8s => None,
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
            HybridNodeServiceName::L1 => None,
            HybridNodeServiceName::Mempool => None,
            HybridNodeServiceName::SierraCompiler => None,
        }
    }

    fn has_p2p_interface(&self) -> bool {
        match self {
            HybridNodeServiceName::Core | HybridNodeServiceName::Mempool => true,
            HybridNodeServiceName::HttpServer
            | HybridNodeServiceName::Gateway
            | HybridNodeServiceName::L1
            | HybridNodeServiceName::SierraCompiler => false,
        }
    }

    fn get_storage(&self, environment: &Environment) -> Option<usize> {
        match environment {
            Environment::CloudK8s(_) => match self {
                HybridNodeServiceName::Core => Some(CORE_STORAGE),
                HybridNodeServiceName::HttpServer
                | HybridNodeServiceName::Gateway
                | HybridNodeServiceName::L1
                | HybridNodeServiceName::Mempool
                | HybridNodeServiceName::SierraCompiler => None,
            },
            Environment::LocalK8s => match self {
                HybridNodeServiceName::Core => Some(TEST_CORE_STORAGE),
                HybridNodeServiceName::HttpServer
                | HybridNodeServiceName::Gateway
                | HybridNodeServiceName::L1
                | HybridNodeServiceName::Mempool
                | HybridNodeServiceName::SierraCompiler => None,
            },
        }
    }

    fn get_resources(&self, environment: &Environment) -> Resources {
        match environment {
            Environment::CloudK8s(cloud_env) => match cloud_env {
                CloudK8sEnvironment::SepoliaIntegration | CloudK8sEnvironment::UpgradeTest => {
                    match self {
                        HybridNodeServiceName::Core => {
                            Resources::new(Resource::new(2, 4), Resource::new(7, 14))
                        }
                        HybridNodeServiceName::HttpServer => {
                            Resources::new(Resource::new(1, 2), Resource::new(4, 8))
                        }
                        HybridNodeServiceName::Gateway => {
                            Resources::new(Resource::new(1, 2), Resource::new(2, 4))
                        }
                        HybridNodeServiceName::L1 => {
                            Resources::new(Resource::new(1, 2), Resource::new(2, 4))
                        }
                        HybridNodeServiceName::Mempool => {
                            Resources::new(Resource::new(1, 2), Resource::new(2, 4))
                        }
                        HybridNodeServiceName::SierraCompiler => {
                            Resources::new(Resource::new(1, 2), Resource::new(2, 4))
                        }
                    }
                }
                CloudK8sEnvironment::Potc2
                | CloudK8sEnvironment::Mainnet
                | CloudK8sEnvironment::SepoliaTestnet
                | CloudK8sEnvironment::StressTest => match self {
                    HybridNodeServiceName::Core => {
                        Resources::new(Resource::new(50, 200), Resource::new(50, 220))
                    }
                    HybridNodeServiceName::HttpServer => {
                        Resources::new(Resource::new(1, 2), Resource::new(4, 8))
                    }
                    HybridNodeServiceName::Gateway => {
                        Resources::new(Resource::new(1, 2), Resource::new(2, 4))
                    }
                    HybridNodeServiceName::L1 => {
                        Resources::new(Resource::new(1, 2), Resource::new(2, 4))
                    }
                    HybridNodeServiceName::Mempool => {
                        Resources::new(Resource::new(1, 2), Resource::new(2, 4))
                    }
                    HybridNodeServiceName::SierraCompiler => {
                        Resources::new(Resource::new(1, 2), Resource::new(2, 4))
                    }
                },
            },
            Environment::LocalK8s => Resources::new(Resource::new(1, 2), Resource::new(4, 8)),
        }
    }

    fn get_replicas(&self, environment: &Environment) -> usize {
        match environment {
            Environment::CloudK8s(_) => match self {
                HybridNodeServiceName::Core => 1,
                HybridNodeServiceName::HttpServer => 1,
                HybridNodeServiceName::Gateway => 2,
                HybridNodeServiceName::L1 => 1,
                HybridNodeServiceName::Mempool => 1,
                HybridNodeServiceName::SierraCompiler => 2,
            },
            Environment::LocalK8s => 1,
        }
    }

    fn get_anti_affinity(&self, environment: &Environment) -> bool {
        match environment {
            Environment::CloudK8s(_) => match self {
                HybridNodeServiceName::Core => true,
                HybridNodeServiceName::HttpServer => false,
                HybridNodeServiceName::Gateway => false,
                HybridNodeServiceName::L1 => false,
                HybridNodeServiceName::Mempool => true,
                HybridNodeServiceName::SierraCompiler => false,
            },
            Environment::LocalK8s => false,
        }
    }

    fn get_service_ports(&self) -> BTreeSet<ServicePort> {
        let mut service_ports = BTreeSet::new();

        match self {
            HybridNodeServiceName::Core => {
                for service_port in ServicePort::iter() {
                    match service_port {
                        ServicePort::BusinessLogic(bl_port) => match bl_port {
                            BusinessLogicServicePort::MonitoringEndpoint
                            | BusinessLogicServicePort::ConsensusManager => {
                                service_ports.insert(service_port);
                            }
                            BusinessLogicServicePort::HttpServer
                            | BusinessLogicServicePort::MempoolP2p => {}
                        },
                        ServicePort::Infra(infra_port) => match infra_port {
                            InfraServicePort::Batcher
                            | InfraServicePort::ClassManager
                            | InfraServicePort::StateSync => {
                                service_ports.insert(service_port);
                            }
                            InfraServicePort::Gateway
                            | InfraServicePort::L1EndpointMonitor
                            | InfraServicePort::L1GasPriceProvider
                            | InfraServicePort::L1Provider
                            | InfraServicePort::Mempool
                            | InfraServicePort::SignatureManager
                            | InfraServicePort::SierraCompiler => {}
                        },
                    }
                }
            }
            HybridNodeServiceName::HttpServer => {
                for service_port in ServicePort::iter() {
                    match service_port {
                        ServicePort::BusinessLogic(bl_port) => match bl_port {
                            BusinessLogicServicePort::MonitoringEndpoint
                            | BusinessLogicServicePort::HttpServer => {
                                service_ports.insert(service_port);
                            }
                            BusinessLogicServicePort::ConsensusManager
                            | BusinessLogicServicePort::MempoolP2p => {}
                        },
                        ServicePort::Infra(infra_port) => match infra_port {
                            InfraServicePort::Batcher
                            | InfraServicePort::ClassManager
                            | InfraServicePort::L1EndpointMonitor
                            | InfraServicePort::L1GasPriceProvider
                            | InfraServicePort::L1Provider
                            | InfraServicePort::StateSync
                            | InfraServicePort::Mempool
                            | InfraServicePort::Gateway
                            | InfraServicePort::SignatureManager
                            | InfraServicePort::SierraCompiler => {}
                        },
                    }
                }
            }
            HybridNodeServiceName::Gateway => {
                for service_port in ServicePort::iter() {
                    match service_port {
                        ServicePort::BusinessLogic(bl_port) => match bl_port {
                            BusinessLogicServicePort::MonitoringEndpoint => {
                                service_ports.insert(service_port);
                            }
                            BusinessLogicServicePort::HttpServer
                            | BusinessLogicServicePort::ConsensusManager
                            | BusinessLogicServicePort::MempoolP2p => {}
                        },
                        ServicePort::Infra(infra_port) => match infra_port {
                            InfraServicePort::Gateway => {
                                service_ports.insert(service_port);
                            }
                            InfraServicePort::Batcher
                            | InfraServicePort::ClassManager
                            | InfraServicePort::L1EndpointMonitor
                            | InfraServicePort::L1GasPriceProvider
                            | InfraServicePort::L1Provider
                            | InfraServicePort::StateSync
                            | InfraServicePort::Mempool
                            | InfraServicePort::SignatureManager
                            | InfraServicePort::SierraCompiler => {}
                        },
                    }
                }
            }
            HybridNodeServiceName::L1 => {
                for service_port in ServicePort::iter() {
                    match service_port {
                        ServicePort::BusinessLogic(bl_port) => match bl_port {
                            BusinessLogicServicePort::MonitoringEndpoint => {
                                service_ports.insert(service_port);
                            }
                            BusinessLogicServicePort::HttpServer
                            | BusinessLogicServicePort::ConsensusManager
                            | BusinessLogicServicePort::MempoolP2p => {}
                        },
                        ServicePort::Infra(infra_port) => match infra_port {
                            InfraServicePort::L1EndpointMonitor
                            | InfraServicePort::L1GasPriceProvider
                            | InfraServicePort::L1Provider => {
                                service_ports.insert(service_port);
                            }
                            InfraServicePort::Batcher
                            | InfraServicePort::ClassManager
                            | InfraServicePort::StateSync
                            | InfraServicePort::Mempool
                            | InfraServicePort::Gateway
                            | InfraServicePort::SignatureManager
                            | InfraServicePort::SierraCompiler => {}
                        },
                    }
                }
            }
            HybridNodeServiceName::Mempool => {
                for service_port in ServicePort::iter() {
                    match service_port {
                        ServicePort::BusinessLogic(bl_port) => match bl_port {
                            BusinessLogicServicePort::MonitoringEndpoint => {
                                service_ports.insert(service_port);
                            }
                            BusinessLogicServicePort::HttpServer
                            | BusinessLogicServicePort::ConsensusManager
                            | BusinessLogicServicePort::MempoolP2p => {}
                        },
                        ServicePort::Infra(infra_port) => match infra_port {
                            InfraServicePort::Mempool => {
                                service_ports.insert(service_port);
                            }
                            InfraServicePort::Batcher
                            | InfraServicePort::ClassManager
                            | InfraServicePort::L1EndpointMonitor
                            | InfraServicePort::L1GasPriceProvider
                            | InfraServicePort::L1Provider
                            | InfraServicePort::StateSync
                            | InfraServicePort::Gateway
                            | InfraServicePort::SignatureManager
                            | InfraServicePort::SierraCompiler => {}
                        },
                    }
                }
            }
            HybridNodeServiceName::SierraCompiler => {
                for service_port in ServicePort::iter() {
                    match service_port {
                        ServicePort::BusinessLogic(bl_port) => match bl_port {
                            BusinessLogicServicePort::MonitoringEndpoint => {
                                service_ports.insert(service_port);
                            }
                            BusinessLogicServicePort::HttpServer
                            | BusinessLogicServicePort::ConsensusManager
                            | BusinessLogicServicePort::MempoolP2p => {}
                        },
                        ServicePort::Infra(infra_port) => match infra_port {
                            InfraServicePort::SierraCompiler => {
                                service_ports.insert(service_port);
                            }
                            InfraServicePort::Batcher
                            | InfraServicePort::ClassManager
                            | InfraServicePort::L1EndpointMonitor
                            | InfraServicePort::L1GasPriceProvider
                            | InfraServicePort::L1Provider
                            | InfraServicePort::StateSync
                            | InfraServicePort::Mempool
                            | InfraServicePort::Gateway
                            | InfraServicePort::SignatureManager => {}
                        },
                    }
                }
            }
        }
        service_ports
    }

    fn get_components_in_service(&self) -> BTreeSet<ComponentConfigInService> {
        let mut components = BTreeSet::new();
        match self {
            HybridNodeServiceName::Core => {
                for component_config_in_service in ComponentConfigInService::iter() {
                    match component_config_in_service {
                        ComponentConfigInService::Batcher
                        | ComponentConfigInService::ClassManager
                        | ComponentConfigInService::Consensus
                        | ComponentConfigInService::General
                        | ComponentConfigInService::MonitoringEndpoint
                        | ComponentConfigInService::StateSync => {
                            components.insert(component_config_in_service);
                        }
                        ComponentConfigInService::BaseLayer
                        | ComponentConfigInService::Gateway
                        | ComponentConfigInService::HttpServer
                        | ComponentConfigInService::L1EndpointMonitor
                        | ComponentConfigInService::L1GasPriceProvider
                        | ComponentConfigInService::L1GasPriceScraper
                        | ComponentConfigInService::L1Provider
                        | ComponentConfigInService::L1Scraper
                        | ComponentConfigInService::Mempool
                        | ComponentConfigInService::MempoolP2p
                        | ComponentConfigInService::SierraCompiler => {}
                    }
                }
            }
            HybridNodeServiceName::HttpServer => {
                for component_config_in_service in ComponentConfigInService::iter() {
                    match component_config_in_service {
                        ComponentConfigInService::General
                        | ComponentConfigInService::HttpServer
                        | ComponentConfigInService::MonitoringEndpoint => {
                            components.insert(component_config_in_service);
                        }
                        ComponentConfigInService::BaseLayer
                        | ComponentConfigInService::Batcher
                        | ComponentConfigInService::ClassManager
                        | ComponentConfigInService::Consensus
                        | ComponentConfigInService::Gateway
                        | ComponentConfigInService::L1EndpointMonitor
                        | ComponentConfigInService::L1GasPriceProvider
                        | ComponentConfigInService::L1GasPriceScraper
                        | ComponentConfigInService::L1Provider
                        | ComponentConfigInService::L1Scraper
                        | ComponentConfigInService::Mempool
                        | ComponentConfigInService::MempoolP2p
                        | ComponentConfigInService::SierraCompiler
                        | ComponentConfigInService::StateSync => {}
                    }
                }
            }
            HybridNodeServiceName::Gateway => {
                for component_config_in_service in ComponentConfigInService::iter() {
                    match component_config_in_service {
                        ComponentConfigInService::Gateway
                        | ComponentConfigInService::General
                        | ComponentConfigInService::MonitoringEndpoint => {
                            components.insert(component_config_in_service);
                        }
                        ComponentConfigInService::BaseLayer
                        | ComponentConfigInService::Batcher
                        | ComponentConfigInService::ClassManager
                        | ComponentConfigInService::Consensus
                        | ComponentConfigInService::HttpServer
                        | ComponentConfigInService::L1EndpointMonitor
                        | ComponentConfigInService::L1GasPriceProvider
                        | ComponentConfigInService::L1GasPriceScraper
                        | ComponentConfigInService::L1Provider
                        | ComponentConfigInService::L1Scraper
                        | ComponentConfigInService::Mempool
                        | ComponentConfigInService::MempoolP2p
                        | ComponentConfigInService::SierraCompiler
                        | ComponentConfigInService::StateSync => {}
                    }
                }
            }
            HybridNodeServiceName::L1 => {
                for component_config_in_service in ComponentConfigInService::iter() {
                    match component_config_in_service {
                        ComponentConfigInService::BaseLayer
                        | ComponentConfigInService::General
                        | ComponentConfigInService::L1EndpointMonitor
                        | ComponentConfigInService::L1GasPriceProvider
                        | ComponentConfigInService::L1GasPriceScraper
                        | ComponentConfigInService::L1Provider
                        | ComponentConfigInService::L1Scraper
                        | ComponentConfigInService::MonitoringEndpoint => {
                            components.insert(component_config_in_service);
                        }
                        ComponentConfigInService::Batcher
                        | ComponentConfigInService::ClassManager
                        | ComponentConfigInService::Consensus
                        | ComponentConfigInService::Gateway
                        | ComponentConfigInService::HttpServer
                        | ComponentConfigInService::Mempool
                        | ComponentConfigInService::MempoolP2p
                        | ComponentConfigInService::SierraCompiler
                        | ComponentConfigInService::StateSync => {}
                    }
                }
            }
            HybridNodeServiceName::Mempool => {
                for component_config_in_service in ComponentConfigInService::iter() {
                    match component_config_in_service {
                        ComponentConfigInService::General
                        | ComponentConfigInService::Mempool
                        | ComponentConfigInService::MempoolP2p
                        | ComponentConfigInService::MonitoringEndpoint => {
                            components.insert(component_config_in_service);
                        }
                        ComponentConfigInService::BaseLayer
                        | ComponentConfigInService::Batcher
                        | ComponentConfigInService::ClassManager
                        | ComponentConfigInService::Consensus
                        | ComponentConfigInService::Gateway
                        | ComponentConfigInService::HttpServer
                        | ComponentConfigInService::L1EndpointMonitor
                        | ComponentConfigInService::L1GasPriceProvider
                        | ComponentConfigInService::L1GasPriceScraper
                        | ComponentConfigInService::L1Provider
                        | ComponentConfigInService::L1Scraper
                        | ComponentConfigInService::SierraCompiler
                        | ComponentConfigInService::StateSync => {}
                    }
                }
            }
            HybridNodeServiceName::SierraCompiler => {
                for component_config_in_service in ComponentConfigInService::iter() {
                    match component_config_in_service {
                        ComponentConfigInService::General
                        | ComponentConfigInService::MonitoringEndpoint
                        | ComponentConfigInService::SierraCompiler => {
                            components.insert(component_config_in_service);
                        }
                        ComponentConfigInService::BaseLayer
                        | ComponentConfigInService::Batcher
                        | ComponentConfigInService::ClassManager
                        | ComponentConfigInService::Consensus
                        | ComponentConfigInService::Gateway
                        | ComponentConfigInService::HttpServer
                        | ComponentConfigInService::L1EndpointMonitor
                        | ComponentConfigInService::L1GasPriceProvider
                        | ComponentConfigInService::L1GasPriceScraper
                        | ComponentConfigInService::L1Provider
                        | ComponentConfigInService::L1Scraper
                        | ComponentConfigInService::Mempool
                        | ComponentConfigInService::MempoolP2p
                        | ComponentConfigInService::StateSync => {}
                    }
                }
            }
        }
        components
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
            | HybridNodeServiceName::L1
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
    l1_gas_price_provider_remote_config: ReactiveComponentExecutionConfig,
    l1_provider_remote_config: ReactiveComponentExecutionConfig,
    state_sync_local_config: ReactiveComponentExecutionConfig,
    mempool_remote_config: ReactiveComponentExecutionConfig,
    sierra_compiler_remote_config: ReactiveComponentExecutionConfig,
    signature_manager_remote_config: ReactiveComponentExecutionConfig,
) -> ComponentConfig {
    let mut config = ComponentConfig::disabled();
    config.batcher = batcher_local_config;
    config.class_manager = class_manager_local_config;
    config.consensus_manager = ActiveComponentExecutionConfig::enabled();
    config.l1_gas_price_provider = l1_gas_price_provider_remote_config;
    config.l1_provider = l1_provider_remote_config;
    config.sierra_compiler = sierra_compiler_remote_config;
    config.signature_manager = signature_manager_remote_config;
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

fn get_l1_component_config(
    l1_gas_price_provider_local_config: ReactiveComponentExecutionConfig,
    l1_provider_local_config: ReactiveComponentExecutionConfig,
    batcher_remote_config: ReactiveComponentExecutionConfig,
    state_sync_remote_config: ReactiveComponentExecutionConfig,
) -> ComponentConfig {
    let mut config = ComponentConfig::disabled();
    config.batcher = batcher_remote_config;
    config.l1_gas_price_provider = l1_gas_price_provider_local_config;
    config.l1_gas_price_scraper = ActiveComponentExecutionConfig::enabled();
    config.l1_provider = l1_provider_local_config;
    config.l1_scraper = ActiveComponentExecutionConfig::enabled();
    config.l1_endpoint_monitor = ReactiveComponentExecutionConfig::local_with_remote_disabled();
    config.monitoring_endpoint = ActiveComponentExecutionConfig::enabled();
    config.state_sync = state_sync_remote_config;
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

/// Loads the hybrid deployments from the given input file and returns a vector of `Deployment`.
pub(crate) fn load_and_create_hybrid_deployments(input_file: &str) -> Vec<Deployment> {
    let inputs =
        DeploymentInputs::load_from_file(resolve_project_relative_path(input_file).unwrap());
    hybrid_deployments(&inputs)
}

fn hybrid_deployments(inputs: &DeploymentInputs) -> Vec<Deployment> {
    inputs
        .node_ids
        .iter()
        .map(|&i| {
            let k8s_service_config_params = if inputs.requires_k8s_service_config_params {
                Some(K8sServiceConfigParams::new(
                    inputs.node_namespace_format.format(&[&i]),
                    inputs.ingress_domain.clone(),
                    inputs.p2p_communication_type,
                ))
            } else {
                None
            };
            hybrid_deployment(
                i,
                inputs.p2p_communication_type,
                inputs.deployment_environment.clone(),
                &Template::new(INSTANCE_NAME_FORMAT),
                &inputs.secret_name_format,
                DeploymentConfigOverride::new(
                    &inputs.starknet_contract_address,
                    &inputs.chain_id_string,
                    &inputs.eth_fee_token_address,
                    inputs.starknet_gateway_url.clone(),
                    &inputs.strk_fee_token_address,
                    inputs.l1_startup_height_override,
                    inputs.node_ids.len(),
                    inputs.state_sync_type.clone(),
                ),
                &inputs.node_namespace_format,
                &inputs.ingress_domain,
                &inputs.http_server_ingress_alternative_name,
                k8s_service_config_params,
            )
        })
        .collect()
}

// TODO(Tsabary): unify these into inner structs.
#[allow(clippy::too_many_arguments)]
pub(crate) fn hybrid_deployment(
    id: usize,
    p2p_communication_type: P2PCommunicationType,
    environment: Environment,
    instance_name_format: &Template,
    secret_name_format: &Template,
    deployment_config_override: DeploymentConfigOverride,
    node_namespace_format: &Template,
    ingress_domain: &str,
    http_server_ingress_alternative_name: &str,
    k8s_service_config_params: Option<K8sServiceConfigParams>,
) -> Deployment {
    Deployment::new(
        NodeType::Hybrid,
        environment,
        &instance_name_format.format(&[&id]),
        Some(ExternalSecret::new(secret_name_format.format(&[&id]))),
        ConfigOverride::new(
            deployment_config_override,
            create_hybrid_instance_config_override(
                id,
                node_namespace_format,
                p2p_communication_type,
                ingress_domain,
            ),
        ),
        IngressParams::new(
            ingress_domain.to_string(),
            Some(vec![http_server_ingress_alternative_name.into()]),
        ),
        k8s_service_config_params,
    )
}

pub(crate) fn create_hybrid_instance_config_override(
    node_id: usize,
    node_namespace_format: &Template,
    p2p_communication_type: P2PCommunicationType,
    domain: &str,
) -> InstanceConfigOverride {
    assert!(
        node_id < MAX_NODE_ID,
        "Node node_id {node_id} exceeds the number of nodes {MAX_NODE_ID}"
    );

    // TODO(Tsabary): these ports should be derived from the hybrid deployment module, and used
    // consistently throughout the code.
    const CORE_SERVICE_PORT: u16 = 53080;
    const MEMPOOL_SERVICE_PORT: u16 = 53200;

    let bootstrap_node_id = 0;
    let bootstrap_peer_id = get_peer_id(bootstrap_node_id);
    let node_peer_id = get_peer_id(node_id);

    let sanitized_domain = p2p_communication_type.get_p2p_domain(domain);

    let build_peer_address =
        |node_service: HybridNodeServiceName, port: u16, node_id: usize, peer_id: &str| {
            let domain = build_service_namespace_domain_address(
                &node_service.k8s_service_name(),
                &node_namespace_format.format(&[&node_id]),
                &sanitized_domain,
            );
            Some(get_p2p_address(&domain, port, peer_id))
        };

    let (consensus_bootstrap_peer_multiaddr, mempool_bootstrap_peer_multiaddr) = match node_id {
        0 => {
            // First node does not have a bootstrap peer.
            (None, None)
        }
        _ => {
            // Other nodes have the first node as a bootstrap peer.
            (
                build_peer_address(
                    HybridNodeServiceName::Core,
                    CORE_SERVICE_PORT,
                    bootstrap_node_id,
                    &bootstrap_peer_id,
                ),
                build_peer_address(
                    HybridNodeServiceName::Mempool,
                    MEMPOOL_SERVICE_PORT,
                    bootstrap_node_id,
                    &bootstrap_peer_id,
                ),
            )
        }
    };

    let (consensus_advertised_multiaddr, mempool_advertised_multiaddr) =
        match p2p_communication_type {
            P2PCommunicationType::Internal =>
            // No advertised addresses for internal communication.
            {
                (None, None)
            }
            P2PCommunicationType::External =>
            // Advertised addresses for external communication.
            {
                (
                    build_peer_address(
                        HybridNodeServiceName::Core,
                        CORE_SERVICE_PORT,
                        node_id,
                        &node_peer_id,
                    ),
                    build_peer_address(
                        HybridNodeServiceName::Mempool,
                        MEMPOOL_SERVICE_PORT,
                        node_id,
                        &node_peer_id,
                    ),
                )
            }
        };

    InstanceConfigOverride::new(
        NetworkConfigOverride::new(
            consensus_bootstrap_peer_multiaddr,
            consensus_advertised_multiaddr,
        ),
        NetworkConfigOverride::new(mempool_bootstrap_peer_multiaddr, mempool_advertised_multiaddr),
        get_validator_id(node_id),
    )
}
