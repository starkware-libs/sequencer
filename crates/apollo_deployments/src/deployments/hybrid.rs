use std::collections::{BTreeMap, BTreeSet, HashMap};

use apollo_infra::component_client::DEFAULT_RETRIES;
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
    Environment,
    InfraServicePort,
    ServicePort,
};
use crate::deployments::distributed::RETRIES_FOR_L1_SERVICES;
use crate::k8s::{Controller, Ingress, IngressParams, Resource, Resources, Toleration};
use crate::scale_policy::ScalePolicy;
use crate::service::{GetComponentConfigs, NodeService, ServiceNameInner};
use crate::update_strategy::UpdateStrategy;
use crate::utils::validate_ports;

pub const HYBRID_NODE_REQUIRED_PORTS_NUM: usize = 10;

const TEST_CORE_STORAGE: usize = 1;

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
    fn get_component_configs(ports: Option<Vec<u16>>) -> HashMap<NodeService, ComponentConfig> {
        let mut component_config_map = HashMap::<NodeService, ComponentConfig>::new();

        let mut service_ports: BTreeMap<InfraServicePort, u16> = BTreeMap::new();
        match ports {
            Some(ports) => {
                validate_ports(&ports, InfraServicePort::iter().count());
                // TODO(Nadin): This should compare against HybridServicePort-specific infra ports,
                // not all InfraServicePort variants.
                for (service_port, port) in InfraServicePort::iter().zip(ports) {
                    service_ports.insert(service_port, port);
                }
            }
            None => {
                // Extract the infra service ports for all inner services of the hybrid node.
                for inner_service_name in Self::iter() {
                    let inner_service_port = inner_service_name.get_infra_service_port_mapping();
                    service_ports.extend(inner_service_port);
                }
            }
        };

<<<<<<< HEAD
        // TODO(Yoav): Add committer when it is ready.
        let batcher = Self::Core.component_config_pair(service_ports[&InfraServicePort::Batcher]);
        let class_manager =
            Self::Core.component_config_pair(service_ports[&InfraServicePort::ClassManager]);
        let gateway =
            Self::Gateway.component_config_pair(service_ports[&InfraServicePort::Gateway]);
        let l1_gas_price_provider =
            Self::L1.component_config_pair(service_ports[&InfraServicePort::L1GasPriceProvider]);
        let l1_provider =
            Self::L1.component_config_pair(service_ports[&InfraServicePort::L1Provider]);
        let l1_endpoint_monitor =
            Self::L1.component_config_pair(service_ports[&InfraServicePort::L1EndpointMonitor]);
        let mempool =
            Self::Mempool.component_config_pair(service_ports[&InfraServicePort::Mempool]);
        let sierra_compiler = Self::SierraCompiler
||||||| dd2fc66ab
        let batcher = HybridNodeServiceName::Core
            .component_config_pair(service_ports[&InfraServicePort::Batcher]);
        let class_manager = HybridNodeServiceName::Core
            .component_config_pair(service_ports[&InfraServicePort::ClassManager]);
        let gateway = HybridNodeServiceName::Gateway
            .component_config_pair(service_ports[&InfraServicePort::Gateway]);
        let l1_gas_price_provider = HybridNodeServiceName::L1
            .component_config_pair(service_ports[&InfraServicePort::L1GasPriceProvider]);
        let l1_provider = HybridNodeServiceName::L1
            .component_config_pair(service_ports[&InfraServicePort::L1Provider]);
        let l1_endpoint_monitor = HybridNodeServiceName::L1
            .component_config_pair(service_ports[&InfraServicePort::L1EndpointMonitor]);
        let mempool = HybridNodeServiceName::Mempool
            .component_config_pair(service_ports[&InfraServicePort::Mempool]);
        let sierra_compiler = HybridNodeServiceName::SierraCompiler
=======
        let batcher = HybridNodeServiceName::Core
            .component_config_pair(service_ports[&InfraServicePort::Batcher]);
        let class_manager = HybridNodeServiceName::Core
            .component_config_pair(service_ports[&InfraServicePort::ClassManager]);
        let gateway = HybridNodeServiceName::Gateway
            .component_config_pair(service_ports[&InfraServicePort::Gateway]);
        let l1_gas_price_provider = HybridNodeServiceName::L1
            .component_config_pair(service_ports[&InfraServicePort::L1GasPriceProvider]);
        let l1_provider = HybridNodeServiceName::L1
            .component_config_pair(service_ports[&InfraServicePort::L1Provider]);
        let mempool = HybridNodeServiceName::Mempool
            .component_config_pair(service_ports[&InfraServicePort::Mempool]);
        let sierra_compiler = HybridNodeServiceName::SierraCompiler
>>>>>>> origin/main-v0.14.1
            .component_config_pair(service_ports[&InfraServicePort::SierraCompiler]);
        let signature_manager =
            Self::Core.component_config_pair(service_ports[&InfraServicePort::SignatureManager]);
        let state_sync =
            Self::Core.component_config_pair(service_ports[&InfraServicePort::StateSync]);

        for inner_service_name in Self::iter() {
            let component_config = match inner_service_name {
                Self::Core => get_core_component_config(
                    batcher.local(),
                    class_manager.local(),
                    l1_gas_price_provider.remote(),
                    l1_provider.remote(),
                    state_sync.local(),
                    mempool.remote(),
                    sierra_compiler.remote(),
                    signature_manager.local(),
                ),
                Self::HttpServer => get_http_server_component_config(gateway.remote()),
                Self::Gateway => get_gateway_component_config(
                    gateway.local(),
                    class_manager.remote(),
                    mempool.remote(),
                    state_sync.remote(),
                ),
                Self::L1 => get_l1_component_config(
                    l1_gas_price_provider.local(),
                    l1_provider.local(),
                    batcher.remote(),
                    state_sync.remote(),
                ),
                Self::Mempool => get_mempool_component_config(
                    mempool.local(),
                    class_manager.remote(),
                    gateway.remote(),
                ),
                Self::SierraCompiler => {
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
            Self::Core => Controller::StatefulSet,
            Self::HttpServer => Controller::Deployment,
            Self::Gateway => Controller::Deployment,
            Self::L1 => Controller::Deployment,
            Self::Mempool => Controller::Deployment,
            Self::SierraCompiler => Controller::Deployment,
        }
    }

    fn get_scale_policy(&self) -> ScalePolicy {
        match self {
            Self::Core | Self::HttpServer | Self::L1 | Self::Mempool => {
                ScalePolicy::StaticallyScaled
            }

            Self::Gateway | Self::SierraCompiler => ScalePolicy::AutoScaled,
        }
    }

    fn get_retries(&self) -> usize {
        match self {
            Self::Core
            | Self::HttpServer
            | Self::Mempool
            | Self::Gateway
            | Self::SierraCompiler => DEFAULT_RETRIES,
            Self::L1 => RETRIES_FOR_L1_SERVICES,
        }
    }

<<<<<<< HEAD
    fn get_toleration(&self, environment: &Environment) -> Option<Toleration> {
        match environment {
            Environment::CloudK8s(cloud_env) => match self {
                Self::Core => match cloud_env {
                    CloudK8sEnvironment::SepoliaIntegration | CloudK8sEnvironment::UpgradeTest => {
                        Some(Toleration::ApolloCoreService)
                    }
                    CloudK8sEnvironment::Mainnet | CloudK8sEnvironment::SepoliaTestnet => {
                        Some(Toleration::ApolloCoreServiceC2D56)
                    }
                },
                Self::HttpServer | Self::Gateway | Self::SierraCompiler => {
                    Some(Toleration::ApolloGeneralService)
                }
                Self::L1 => Some(Toleration::ApolloL1Service),
                Self::Mempool => Some(Toleration::ApolloMempoolService),
            },
            Environment::LocalK8s => None,
        }
||||||| dd2fc66ab
    fn get_toleration(&self, environment: &Environment) -> Option<Toleration> {
        match environment {
            Environment::CloudK8s(cloud_env) => match self {
                HybridNodeServiceName::Core => match cloud_env {
                    CloudK8sEnvironment::SepoliaIntegration | CloudK8sEnvironment::UpgradeTest => {
                        Some(Toleration::ApolloCoreService)
                    }
                    CloudK8sEnvironment::Mainnet | CloudK8sEnvironment::SepoliaTestnet => {
                        Some(Toleration::ApolloCoreServiceC2D56)
                    }
                },
                HybridNodeServiceName::HttpServer
                | HybridNodeServiceName::Gateway
                | HybridNodeServiceName::SierraCompiler => Some(Toleration::ApolloGeneralService),
                HybridNodeServiceName::L1 => Some(Toleration::ApolloL1Service),
                HybridNodeServiceName::Mempool => Some(Toleration::ApolloMempoolService),
            },
            Environment::LocalK8s => None,
        }
=======
    fn get_toleration(&self, _environment: &Environment) -> Option<Toleration> {
        None
>>>>>>> origin/main-v0.14.1
    }

    fn get_ingress(
        &self,
        _environment: &Environment,
        _ingress_params: IngressParams,
    ) -> Option<Ingress> {
<<<<<<< HEAD
        match self {
            Self::Core | Self::Gateway | Self::L1 | Self::Mempool | Self::SierraCompiler => None,
            Self::HttpServer => match &environment {
                Environment::CloudK8s(_) => {
                    get_ingress(ingress_params, get_environment_ingress_internal(environment))
                }
                Environment::LocalK8s => None,
            },
        }
||||||| dd2fc66ab
        match self {
            HybridNodeServiceName::Core
            | HybridNodeServiceName::Gateway
            | HybridNodeServiceName::L1
            | HybridNodeServiceName::Mempool
            | HybridNodeServiceName::SierraCompiler => None,
            HybridNodeServiceName::HttpServer => match &environment {
                Environment::CloudK8s(_) => {
                    get_ingress(ingress_params, get_environment_ingress_internal(environment))
                }
                Environment::LocalK8s => None,
            },
        }
=======
        None
>>>>>>> origin/main-v0.14.1
    }

    fn has_p2p_interface(&self) -> bool {
        match self {
            Self::Core | Self::Mempool => true,
            Self::HttpServer | Self::Gateway | Self::L1 | Self::SierraCompiler => false,
        }
    }

<<<<<<< HEAD
    fn get_storage(&self, environment: &Environment) -> Option<usize> {
        match environment {
            Environment::CloudK8s(_) => match self {
                Self::Core => Some(CORE_STORAGE),
                Self::HttpServer
                | Self::Gateway
                | Self::L1
                | Self::Mempool
                | Self::SierraCompiler => None,
            },
            Environment::LocalK8s => match self {
                Self::Core => Some(TEST_CORE_STORAGE),
                Self::HttpServer
                | Self::Gateway
                | Self::L1
                | Self::Mempool
                | Self::SierraCompiler => None,
            },
||||||| dd2fc66ab
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
=======
    fn get_storage(&self, _environment: &Environment) -> Option<usize> {
        match self {
            HybridNodeServiceName::Core => Some(TEST_CORE_STORAGE),
            HybridNodeServiceName::HttpServer
            | HybridNodeServiceName::Gateway
            | HybridNodeServiceName::L1
            | HybridNodeServiceName::Mempool
            | HybridNodeServiceName::SierraCompiler => None,
>>>>>>> origin/main-v0.14.1
        }
    }

<<<<<<< HEAD
    fn get_resources(&self, environment: &Environment) -> Resources {
        match environment {
            Environment::CloudK8s(cloud_env) => match cloud_env {
                CloudK8sEnvironment::SepoliaIntegration | CloudK8sEnvironment::UpgradeTest => {
                    match self {
                        Self::Core => Resources::new(Resource::new(2, 4), Resource::new(7, 14)),
                        Self::HttpServer => {
                            Resources::new(Resource::new(1, 2), Resource::new(4, 8))
                        }
                        Self::Gateway => Resources::new(Resource::new(1, 2), Resource::new(2, 4)),
                        Self::L1 => Resources::new(Resource::new(1, 2), Resource::new(2, 4)),
                        Self::Mempool => Resources::new(Resource::new(1, 2), Resource::new(2, 4)),
                        Self::SierraCompiler => {
                            Resources::new(Resource::new(1, 2), Resource::new(2, 4))
                        }
                    }
                }
                CloudK8sEnvironment::Mainnet | CloudK8sEnvironment::SepoliaTestnet => match self {
                    Self::Core => Resources::new(Resource::new(50, 200), Resource::new(50, 220)),
                    Self::HttpServer => Resources::new(Resource::new(1, 2), Resource::new(4, 8)),
                    Self::Gateway => Resources::new(Resource::new(1, 2), Resource::new(2, 4)),
                    Self::L1 => Resources::new(Resource::new(2, 4), Resource::new(3, 12)),
                    Self::Mempool => Resources::new(Resource::new(2, 4), Resource::new(3, 12)),
                    Self::SierraCompiler => {
                        Resources::new(Resource::new(1, 2), Resource::new(2, 4))
                    }
                },
            },
            Environment::LocalK8s => Resources::new(Resource::new(1, 2), Resource::new(4, 8)),
        }
||||||| dd2fc66ab
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
                CloudK8sEnvironment::Mainnet | CloudK8sEnvironment::SepoliaTestnet => match self {
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
                        Resources::new(Resource::new(2, 4), Resource::new(3, 12))
                    }
                    HybridNodeServiceName::Mempool => {
                        Resources::new(Resource::new(2, 4), Resource::new(3, 12))
                    }
                    HybridNodeServiceName::SierraCompiler => {
                        Resources::new(Resource::new(1, 2), Resource::new(2, 4))
                    }
                },
            },
            Environment::LocalK8s => Resources::new(Resource::new(1, 2), Resource::new(4, 8)),
        }
=======
    fn get_resources(&self, _environment: &Environment) -> Resources {
        Resources::new(Resource::new(1, 2), Resource::new(4, 8))
>>>>>>> origin/main-v0.14.1
    }

<<<<<<< HEAD
    fn get_replicas(&self, environment: &Environment) -> usize {
        match environment {
            Environment::CloudK8s(_) => match self {
                Self::Core => 1,
                Self::HttpServer => 1,
                Self::Gateway => 2,
                Self::L1 => 1,
                Self::Mempool => 1,
                Self::SierraCompiler => 2,
            },
            Environment::LocalK8s => 1,
        }
||||||| dd2fc66ab
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
=======
    fn get_replicas(&self, _environment: &Environment) -> usize {
        1
>>>>>>> origin/main-v0.14.1
    }

<<<<<<< HEAD
    fn get_anti_affinity(&self, environment: &Environment) -> bool {
        match environment {
            Environment::CloudK8s(_) => match self {
                Self::Core => true,
                Self::HttpServer => false,
                Self::Gateway => false,
                Self::L1 => true,
                Self::Mempool => true,
                Self::SierraCompiler => false,
            },
            Environment::LocalK8s => false,
        }
||||||| dd2fc66ab
    fn get_anti_affinity(&self, environment: &Environment) -> bool {
        match environment {
            Environment::CloudK8s(_) => match self {
                HybridNodeServiceName::Core => true,
                HybridNodeServiceName::HttpServer => false,
                HybridNodeServiceName::Gateway => false,
                HybridNodeServiceName::L1 => true,
                HybridNodeServiceName::Mempool => true,
                HybridNodeServiceName::SierraCompiler => false,
            },
            Environment::LocalK8s => false,
        }
=======
    fn get_anti_affinity(&self, _environment: &Environment) -> bool {
        false
>>>>>>> origin/main-v0.14.1
    }

    fn get_service_ports(&self) -> BTreeSet<ServicePort> {
        let mut service_ports = BTreeSet::new();

        match self {
            Self::Core => {
                for service_port in ServicePort::iter() {
                    match service_port {
                        ServicePort::BusinessLogic(bl_port) => match bl_port {
                            BusinessLogicServicePort::MonitoringEndpoint
                            | BusinessLogicServicePort::ConsensusP2p => {
                                service_ports.insert(service_port);
                            }
                            BusinessLogicServicePort::HttpServer
                            | BusinessLogicServicePort::MempoolP2p => {}
                        },
                        ServicePort::Infra(infra_port) => match infra_port {
                            InfraServicePort::Batcher
                            | InfraServicePort::ClassManager
                            | InfraServicePort::StateSync
                            | InfraServicePort::SignatureManager => {
                                service_ports.insert(service_port);
                            }
                            InfraServicePort::Gateway
                            | InfraServicePort::L1EndpointMonitor
                            | InfraServicePort::L1GasPriceProvider
                            | InfraServicePort::L1Provider
                            | InfraServicePort::Mempool
                            | InfraServicePort::SierraCompiler => {}
                        },
                    }
                }
            }
            Self::HttpServer => {
                for service_port in ServicePort::iter() {
                    match service_port {
                        ServicePort::BusinessLogic(bl_port) => match bl_port {
                            BusinessLogicServicePort::MonitoringEndpoint
                            | BusinessLogicServicePort::HttpServer => {
                                service_ports.insert(service_port);
                            }
                            BusinessLogicServicePort::ConsensusP2p
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
            Self::Gateway => {
                for service_port in ServicePort::iter() {
                    match service_port {
                        ServicePort::BusinessLogic(bl_port) => match bl_port {
                            BusinessLogicServicePort::MonitoringEndpoint => {
                                service_ports.insert(service_port);
                            }
                            BusinessLogicServicePort::HttpServer
                            | BusinessLogicServicePort::ConsensusP2p
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
            Self::L1 => {
                for service_port in ServicePort::iter() {
                    match service_port {
                        ServicePort::BusinessLogic(bl_port) => match bl_port {
                            BusinessLogicServicePort::MonitoringEndpoint => {
                                service_ports.insert(service_port);
                            }
                            BusinessLogicServicePort::HttpServer
                            | BusinessLogicServicePort::ConsensusP2p
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
            Self::Mempool => {
                for service_port in ServicePort::iter() {
                    match service_port {
                        ServicePort::BusinessLogic(bl_port) => match bl_port {
                            BusinessLogicServicePort::MonitoringEndpoint => {
                                service_ports.insert(service_port);
                            }
                            BusinessLogicServicePort::HttpServer
                            | BusinessLogicServicePort::ConsensusP2p
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
            Self::SierraCompiler => {
                for service_port in ServicePort::iter() {
                    match service_port {
                        ServicePort::BusinessLogic(bl_port) => match bl_port {
                            BusinessLogicServicePort::MonitoringEndpoint => {
                                service_ports.insert(service_port);
                            }
                            BusinessLogicServicePort::HttpServer
                            | BusinessLogicServicePort::ConsensusP2p
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
            Self::Core => {
                for component_config_in_service in ComponentConfigInService::iter() {
                    match component_config_in_service {
                        ComponentConfigInService::Batcher
                        | ComponentConfigInService::ClassManager
                        | ComponentConfigInService::Consensus
                        | ComponentConfigInService::ConfigManager
                        | ComponentConfigInService::General
                        | ComponentConfigInService::MonitoringEndpoint
                        | ComponentConfigInService::SignatureManager
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
            Self::HttpServer => {
                for component_config_in_service in ComponentConfigInService::iter() {
                    match component_config_in_service {
                        ComponentConfigInService::ConfigManager
                        | ComponentConfigInService::General
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
                        | ComponentConfigInService::SignatureManager
                        | ComponentConfigInService::StateSync => {}
                    }
                }
            }
            Self::Gateway => {
                for component_config_in_service in ComponentConfigInService::iter() {
                    match component_config_in_service {
                        ComponentConfigInService::ConfigManager
                        | ComponentConfigInService::Gateway
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
                        | ComponentConfigInService::SignatureManager
                        | ComponentConfigInService::StateSync => {}
                    }
                }
            }
            Self::L1 => {
                for component_config_in_service in ComponentConfigInService::iter() {
                    match component_config_in_service {
                        ComponentConfigInService::BaseLayer
                        | ComponentConfigInService::ConfigManager
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
                        | ComponentConfigInService::SignatureManager
                        | ComponentConfigInService::StateSync => {}
                    }
                }
            }
            Self::Mempool => {
                for component_config_in_service in ComponentConfigInService::iter() {
                    match component_config_in_service {
                        ComponentConfigInService::ConfigManager
                        | ComponentConfigInService::General
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
                        | ComponentConfigInService::SignatureManager
                        | ComponentConfigInService::StateSync => {}
                    }
                }
            }
            Self::SierraCompiler => {
                for component_config_in_service in ComponentConfigInService::iter() {
                    match component_config_in_service {
                        ComponentConfigInService::ConfigManager
                        | ComponentConfigInService::General
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
                        | ComponentConfigInService::SignatureManager
                        | ComponentConfigInService::StateSync => {}
                    }
                }
            }
        }
        components
    }

    fn get_update_strategy(&self) -> UpdateStrategy {
        match self {
            Self::Core => UpdateStrategy::RollingUpdate,
            Self::HttpServer => UpdateStrategy::RollingUpdate,
            Self::Gateway => UpdateStrategy::RollingUpdate,
            Self::L1 => UpdateStrategy::Recreate,
            Self::Mempool => UpdateStrategy::Recreate,
            Self::SierraCompiler => UpdateStrategy::RollingUpdate,
        }
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
    config.config_manager = ReactiveComponentExecutionConfig::local_with_remote_disabled();
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
    config.config_manager = ReactiveComponentExecutionConfig::local_with_remote_disabled();
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
    config.config_manager = ReactiveComponentExecutionConfig::local_with_remote_disabled();
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
    config.config_manager = ReactiveComponentExecutionConfig::local_with_remote_disabled();
    config.gateway = gateway_remote_config;
    config.monitoring_endpoint = ActiveComponentExecutionConfig::enabled();
    config
}

fn get_sierra_compiler_component_config(
    sierra_compiler_local_config: ReactiveComponentExecutionConfig,
) -> ComponentConfig {
    let mut config = ComponentConfig::disabled();
    config.sierra_compiler = sierra_compiler_local_config;
    config.config_manager = ReactiveComponentExecutionConfig::local_with_remote_disabled();
    config.monitoring_endpoint = ActiveComponentExecutionConfig::enabled();
    config
}

fn get_http_server_component_config(
    gateway_remote_config: ReactiveComponentExecutionConfig,
) -> ComponentConfig {
    let mut config = ComponentConfig::disabled();
    config.http_server = ActiveComponentExecutionConfig::enabled();
    config.gateway = gateway_remote_config;
    config.config_manager = ReactiveComponentExecutionConfig::local_with_remote_disabled();
    config.monitoring_endpoint = ActiveComponentExecutionConfig::enabled();
    config
}
