use std::collections::{BTreeMap, BTreeSet};
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

use crate::deployment_definitions::{
    BusinessLogicServicePort,
    ComponentConfigInService,
    Environment,
    InfraServicePort,
    ServicePort,
};
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
use crate::service::{GetComponentConfigs, NodeService, ServiceNameInner};
use crate::update_strategy::UpdateStrategy;
use crate::utils::determine_port_numbers;

pub const DISTRIBUTED_NODE_REQUIRED_PORTS_NUM: usize = 10;

const BASE_PORT: u16 = 15000; // TODO(Tsabary): arbitrary port, need to resolve.
const BATCHER_STORAGE: usize = 500;
const CLASS_MANAGER_STORAGE: usize = 500;
const STATE_SYNC_STORAGE: usize = 500;

// TODO(Tsabary): define consts and functions whenever relevant.

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
    SignatureManager,
    StateSync,
}

// Implement conversion from `DistributedNodeServiceName` to `NodeService`
impl From<DistributedNodeServiceName> for NodeService {
    fn from(service: DistributedNodeServiceName) -> Self {
        NodeService::Distributed(service)
    }
}

impl GetComponentConfigs for DistributedNodeServiceName {
    fn get_component_configs(ports: Option<Vec<u16>>) -> IndexMap<NodeService, ComponentConfig> {
        let mut service_ports: BTreeMap<InfraServicePort, u16> = BTreeMap::new();
        match ports {
            Some(ports) => {
                let determined_ports = determine_port_numbers(
                    Some(ports),
                    DISTRIBUTED_NODE_REQUIRED_PORTS_NUM,
                    BASE_PORT,
                );
                for (service_port, port) in InfraServicePort::iter().zip(determined_ports) {
                    service_ports.insert(service_port, port);
                }
            }
            None => {
                // Extract the service ports for all inner services of the distributed node.
                for inner_service_name in DistributedNodeServiceName::iter() {
                    let inner_service_port = inner_service_name.get_infra_service_port_mapping();
                    service_ports.extend(inner_service_port);
                }
            }
        };

        let batcher = DistributedNodeServiceName::Batcher
            .component_config_pair(service_ports[&InfraServicePort::Batcher]);
        let class_manager = DistributedNodeServiceName::ClassManager
            .component_config_pair(service_ports[&InfraServicePort::ClassManager]);
        let gateway = DistributedNodeServiceName::Gateway
            .component_config_pair(service_ports[&InfraServicePort::Gateway]);
        let l1_endpoint_monitor = DistributedNodeServiceName::L1
            .component_config_pair(service_ports[&InfraServicePort::L1EndpointMonitor]);
        let l1_gas_price_provider = DistributedNodeServiceName::L1
            .component_config_pair(service_ports[&InfraServicePort::L1GasPriceProvider]);
        let l1_provider = DistributedNodeServiceName::L1
            .component_config_pair(service_ports[&InfraServicePort::L1Provider]);
        let mempool = DistributedNodeServiceName::Mempool
            .component_config_pair(service_ports[&InfraServicePort::Mempool]);
        let sierra_compiler = DistributedNodeServiceName::SierraCompiler
            .component_config_pair(service_ports[&InfraServicePort::SierraCompiler]);
        let signature_manager = DistributedNodeServiceName::SignatureManager
            .component_config_pair(service_ports[&InfraServicePort::SignatureManager]);
        let state_sync = DistributedNodeServiceName::StateSync
            .component_config_pair(service_ports[&InfraServicePort::StateSync]);

        let mut component_config_map = IndexMap::<NodeService, ComponentConfig>::new();
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
                        signature_manager.remote(),
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
                    l1_endpoint_monitor.local(),
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
                DistributedNodeServiceName::SignatureManager => {
                    get_signature_manager_component_config(signature_manager.local())
                }
                DistributedNodeServiceName::StateSync => {
                    get_state_sync_component_config(state_sync.local(), class_manager.remote())
                }
            };
            let node_service = inner_service_name.into();
            component_config_map.insert(node_service, component_config);
        }
        component_config_map
    }
}

// TODO(Tsabary): per each service, update all values.
impl ServiceNameInner for DistributedNodeServiceName {
    fn get_controller(&self) -> Controller {
        match self {
            DistributedNodeServiceName::Batcher => Controller::StatefulSet,
            DistributedNodeServiceName::ClassManager => Controller::StatefulSet,
            DistributedNodeServiceName::ConsensusManager => Controller::StatefulSet,
            DistributedNodeServiceName::HttpServer => Controller::Deployment,
            DistributedNodeServiceName::Gateway => Controller::Deployment,
            DistributedNodeServiceName::L1 => Controller::Deployment,
            DistributedNodeServiceName::Mempool => Controller::Deployment,
            DistributedNodeServiceName::SierraCompiler => Controller::Deployment,
            DistributedNodeServiceName::StateSync => Controller::StatefulSet,
            // TODO(Nadin): Decide on controller for the SignatureManager.
            DistributedNodeServiceName::SignatureManager => Controller::StatefulSet,
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
            DistributedNodeServiceName::SignatureManager => false,
        }
    }

    fn get_toleration(&self, environment: &Environment) -> Option<Toleration> {
        match environment {
            Environment::CloudK8s(_) => match self {
                DistributedNodeServiceName::Batcher => Some(Toleration::ApolloCoreService),
                DistributedNodeServiceName::ClassManager => Some(Toleration::ApolloGeneralService),
                DistributedNodeServiceName::ConsensusManager => Some(Toleration::ApolloCoreService),
                DistributedNodeServiceName::HttpServer => Some(Toleration::ApolloGeneralService),
                DistributedNodeServiceName::Gateway => Some(Toleration::ApolloGeneralService),
                DistributedNodeServiceName::L1 => Some(Toleration::ApolloL1Service),
                DistributedNodeServiceName::Mempool => Some(Toleration::ApolloMempoolService),
                DistributedNodeServiceName::SierraCompiler => {
                    Some(Toleration::ApolloGeneralService)
                }
                DistributedNodeServiceName::SignatureManager => {
                    Some(Toleration::ApolloGeneralService)
                }
                DistributedNodeServiceName::StateSync => Some(Toleration::ApolloGeneralService),
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
            DistributedNodeServiceName::Batcher => None,
            DistributedNodeServiceName::ClassManager => None,
            DistributedNodeServiceName::ConsensusManager => None,
            DistributedNodeServiceName::HttpServer => {
                get_ingress(ingress_params, get_environment_ingress_internal(environment))
            }
            DistributedNodeServiceName::Gateway => None,
            DistributedNodeServiceName::L1 => None,
            DistributedNodeServiceName::Mempool => None,
            DistributedNodeServiceName::SierraCompiler => None,
            DistributedNodeServiceName::SignatureManager => None,
            DistributedNodeServiceName::StateSync => None,
        }
    }

    fn has_p2p_interface(&self) -> bool {
        match self {
            DistributedNodeServiceName::ConsensusManager
            | DistributedNodeServiceName::Mempool
            | DistributedNodeServiceName::StateSync => true,
            DistributedNodeServiceName::Batcher
            | DistributedNodeServiceName::ClassManager
            | DistributedNodeServiceName::HttpServer
            | DistributedNodeServiceName::Gateway
            | DistributedNodeServiceName::L1
            | DistributedNodeServiceName::SierraCompiler
            | DistributedNodeServiceName::SignatureManager => false,
        }
    }

    fn get_storage(&self, environment: &Environment) -> Option<usize> {
        match environment {
            Environment::CloudK8s(_) => match self {
                DistributedNodeServiceName::Batcher => Some(BATCHER_STORAGE),
                DistributedNodeServiceName::ClassManager => Some(CLASS_MANAGER_STORAGE),
                DistributedNodeServiceName::ConsensusManager => None,
                DistributedNodeServiceName::HttpServer => None,
                DistributedNodeServiceName::Gateway => None,
                DistributedNodeServiceName::L1 => None,
                DistributedNodeServiceName::Mempool => None,
                DistributedNodeServiceName::SierraCompiler => None,
                DistributedNodeServiceName::SignatureManager => None,
                DistributedNodeServiceName::StateSync => Some(STATE_SYNC_STORAGE),
            },
            Environment::LocalK8s => None,
        }
    }

    fn get_resources(&self, _environment: &Environment) -> Resources {
        Resources::new(Resource::new(1, 2), Resource::new(4, 8))
    }

    fn get_replicas(&self, _environment: &Environment) -> usize {
        1
    }

    fn get_anti_affinity(&self, environment: &Environment) -> bool {
        match environment {
            Environment::CloudK8s(_) => match self {
                DistributedNodeServiceName::Batcher => true,
                DistributedNodeServiceName::ClassManager => false,
                DistributedNodeServiceName::ConsensusManager => false,
                DistributedNodeServiceName::HttpServer => false,
                DistributedNodeServiceName::Gateway => false,
                DistributedNodeServiceName::L1 => false,
                DistributedNodeServiceName::Mempool => true,
                DistributedNodeServiceName::SierraCompiler => false,
                DistributedNodeServiceName::SignatureManager => false,
                DistributedNodeServiceName::StateSync => false,
            },
            Environment::LocalK8s => false,
        }
    }

    fn get_service_ports(&self) -> BTreeSet<ServicePort> {
        let mut service_ports = BTreeSet::new();

        match self {
            DistributedNodeServiceName::Batcher => {
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
                            InfraServicePort::Batcher => {
                                service_ports.insert(service_port);
                            }
                            InfraServicePort::ClassManager
                            | InfraServicePort::Gateway
                            | InfraServicePort::L1EndpointMonitor
                            | InfraServicePort::L1GasPriceProvider
                            | InfraServicePort::L1Provider
                            | InfraServicePort::Mempool
                            | InfraServicePort::SignatureManager
                            | InfraServicePort::StateSync
                            | InfraServicePort::SierraCompiler => {}
                        },
                    }
                }
            }
            DistributedNodeServiceName::ClassManager => {
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
                            InfraServicePort::ClassManager => {
                                service_ports.insert(service_port);
                            }
                            InfraServicePort::Batcher
                            | InfraServicePort::Gateway
                            | InfraServicePort::L1EndpointMonitor
                            | InfraServicePort::L1GasPriceProvider
                            | InfraServicePort::L1Provider
                            | InfraServicePort::Mempool
                            | InfraServicePort::SignatureManager
                            | InfraServicePort::StateSync
                            | InfraServicePort::SierraCompiler => {}
                        },
                    }
                }
            }
            DistributedNodeServiceName::ConsensusManager => {
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
                            | InfraServicePort::Gateway
                            | InfraServicePort::L1EndpointMonitor
                            | InfraServicePort::L1GasPriceProvider
                            | InfraServicePort::L1Provider
                            | InfraServicePort::Mempool
                            | InfraServicePort::SignatureManager
                            | InfraServicePort::StateSync
                            | InfraServicePort::SierraCompiler => {}
                        },
                    }
                }
            }
            DistributedNodeServiceName::HttpServer => {
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
                            | InfraServicePort::Gateway
                            | InfraServicePort::L1EndpointMonitor
                            | InfraServicePort::L1GasPriceProvider
                            | InfraServicePort::L1Provider
                            | InfraServicePort::Mempool
                            | InfraServicePort::SignatureManager
                            | InfraServicePort::StateSync
                            | InfraServicePort::SierraCompiler => {}
                        },
                    }
                }
            }

            DistributedNodeServiceName::Gateway => {
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
                            | InfraServicePort::Mempool
                            | InfraServicePort::SignatureManager
                            | InfraServicePort::StateSync
                            | InfraServicePort::SierraCompiler => {}
                        },
                    }
                }
            }
            DistributedNodeServiceName::L1 => {
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
                            | InfraServicePort::Gateway
                            | InfraServicePort::Mempool
                            | InfraServicePort::SignatureManager
                            | InfraServicePort::StateSync
                            | InfraServicePort::SierraCompiler => {}
                        },
                    }
                }
            }
            DistributedNodeServiceName::Mempool => {
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
                            | InfraServicePort::Gateway
                            | InfraServicePort::L1EndpointMonitor
                            | InfraServicePort::L1GasPriceProvider
                            | InfraServicePort::L1Provider
                            | InfraServicePort::SignatureManager
                            | InfraServicePort::StateSync
                            | InfraServicePort::SierraCompiler => {}
                        },
                    }
                }
            }
            DistributedNodeServiceName::SierraCompiler => {
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
                            | InfraServicePort::Gateway
                            | InfraServicePort::L1EndpointMonitor
                            | InfraServicePort::L1GasPriceProvider
                            | InfraServicePort::L1Provider
                            | InfraServicePort::Mempool
                            | InfraServicePort::SignatureManager
                            | InfraServicePort::StateSync => {}
                        },
                    }
                }
            }
            DistributedNodeServiceName::SignatureManager => {
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
                            InfraServicePort::SignatureManager => {
                                service_ports.insert(service_port);
                            }
                            InfraServicePort::Batcher
                            | InfraServicePort::ClassManager
                            | InfraServicePort::Gateway
                            | InfraServicePort::L1EndpointMonitor
                            | InfraServicePort::L1GasPriceProvider
                            | InfraServicePort::L1Provider
                            | InfraServicePort::Mempool
                            | InfraServicePort::StateSync
                            | InfraServicePort::SierraCompiler => {}
                        },
                    }
                }
            }
            DistributedNodeServiceName::StateSync => {
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
                            InfraServicePort::StateSync => {
                                service_ports.insert(service_port);
                            }
                            InfraServicePort::Batcher
                            | InfraServicePort::ClassManager
                            | InfraServicePort::Gateway
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
        }

        service_ports
    }

    // TODO(Tsabary): verify that each service runs the components it should.
    fn get_components_in_service(&self) -> BTreeSet<ComponentConfigInService> {
        let mut components = BTreeSet::new();
        match self {
            DistributedNodeServiceName::Batcher => {
                for component_config_in_service in ComponentConfigInService::iter() {
                    match component_config_in_service {
                        ComponentConfigInService::Batcher
                        | ComponentConfigInService::General
                        | ComponentConfigInService::MonitoringEndpoint => {
                            components.insert(component_config_in_service);
                        }
                        ComponentConfigInService::BaseLayer
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
                        | ComponentConfigInService::SierraCompiler
                        | ComponentConfigInService::SignatureManager
                        | ComponentConfigInService::StateSync => {}
                    }
                }
            }
            DistributedNodeServiceName::ClassManager => {
                for component_config_in_service in ComponentConfigInService::iter() {
                    match component_config_in_service {
                        ComponentConfigInService::ClassManager
                        | ComponentConfigInService::General
                        | ComponentConfigInService::MonitoringEndpoint => {
                            components.insert(component_config_in_service);
                        }
                        ComponentConfigInService::BaseLayer
                        | ComponentConfigInService::Batcher
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
                        | ComponentConfigInService::SierraCompiler
                        | ComponentConfigInService::SignatureManager
                        | ComponentConfigInService::StateSync => {}
                    }
                }
            }
            DistributedNodeServiceName::ConsensusManager => {
                for component_config_in_service in ComponentConfigInService::iter() {
                    match component_config_in_service {
                        ComponentConfigInService::Consensus
                        | ComponentConfigInService::General
                        | ComponentConfigInService::MonitoringEndpoint => {
                            components.insert(component_config_in_service);
                        }
                        ComponentConfigInService::BaseLayer
                        | ComponentConfigInService::Batcher
                        | ComponentConfigInService::ClassManager
                        | ComponentConfigInService::Gateway
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
            DistributedNodeServiceName::HttpServer => {
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
                        | ComponentConfigInService::SignatureManager
                        | ComponentConfigInService::StateSync => {}
                    }
                }
            }
            DistributedNodeServiceName::Gateway => {
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
                        | ComponentConfigInService::SignatureManager
                        | ComponentConfigInService::StateSync => {}
                    }
                }
            }
            DistributedNodeServiceName::L1 => {
                for component_config_in_service in ComponentConfigInService::iter() {
                    match component_config_in_service {
                        ComponentConfigInService::General
                        | ComponentConfigInService::L1EndpointMonitor
                        | ComponentConfigInService::L1GasPriceProvider
                        | ComponentConfigInService::L1GasPriceScraper
                        | ComponentConfigInService::L1Provider
                        | ComponentConfigInService::L1Scraper
                        | ComponentConfigInService::MonitoringEndpoint => {
                            components.insert(component_config_in_service);
                        }
                        ComponentConfigInService::BaseLayer
                        | ComponentConfigInService::Batcher
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
            DistributedNodeServiceName::Mempool => {
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
                        | ComponentConfigInService::SignatureManager
                        | ComponentConfigInService::StateSync => {}
                    }
                }
            }
            DistributedNodeServiceName::SierraCompiler => {
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
                        | ComponentConfigInService::SignatureManager
                        | ComponentConfigInService::StateSync => {}
                    }
                }
            }
            DistributedNodeServiceName::SignatureManager => {
                for component_config_in_service in ComponentConfigInService::iter() {
                    match component_config_in_service {
                        ComponentConfigInService::General
                        | ComponentConfigInService::MonitoringEndpoint
                        | ComponentConfigInService::SignatureManager => {
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
                        | ComponentConfigInService::SierraCompiler
                        | ComponentConfigInService::StateSync => {}
                    }
                }
            }
            DistributedNodeServiceName::StateSync => {
                for component_config_in_service in ComponentConfigInService::iter() {
                    match component_config_in_service {
                        ComponentConfigInService::General
                        | ComponentConfigInService::MonitoringEndpoint
                        | ComponentConfigInService::StateSync => {
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
                        | ComponentConfigInService::SierraCompiler
                        | ComponentConfigInService::SignatureManager => {}
                    }
                }
            }
        }
        components
    }

    fn get_update_strategy(&self) -> UpdateStrategy {
        match self {
            DistributedNodeServiceName::Batcher => UpdateStrategy::RollingUpdate,
            DistributedNodeServiceName::ClassManager => UpdateStrategy::Recreate,
            DistributedNodeServiceName::ConsensusManager => UpdateStrategy::Recreate,
            DistributedNodeServiceName::HttpServer => UpdateStrategy::RollingUpdate,
            DistributedNodeServiceName::Gateway => UpdateStrategy::RollingUpdate,
            DistributedNodeServiceName::L1 => UpdateStrategy::RollingUpdate,
            DistributedNodeServiceName::Mempool => UpdateStrategy::Recreate,
            DistributedNodeServiceName::SierraCompiler => UpdateStrategy::RollingUpdate,
            DistributedNodeServiceName::SignatureManager => UpdateStrategy::Recreate,
            DistributedNodeServiceName::StateSync => UpdateStrategy::Recreate,
        }
    }
}

impl DistributedNodeServiceName {
    // TODO(Tsabary): there's code duplication here that needs to be removed, especially with
    // respect of the hybrid node.

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
            DistributedNodeServiceName::Gateway | DistributedNodeServiceName::SierraCompiler => {
                let remote_client_config_ref = base
                    .remote_client_config
                    .as_mut()
                    .expect("Remote client config should be available");
                remote_client_config_ref.idle_connections = IDLE_CONNECTIONS_FOR_AUTOSCALED_SERVICES
            }
            DistributedNodeServiceName::Batcher
            | DistributedNodeServiceName::ClassManager
            | DistributedNodeServiceName::ConsensusManager
            | DistributedNodeServiceName::HttpServer
            | DistributedNodeServiceName::L1
            | DistributedNodeServiceName::Mempool
            | DistributedNodeServiceName::SignatureManager
            | DistributedNodeServiceName::StateSync => {}
        };
        base
    }

    fn component_config_pair(&self, port: u16) -> DistributedNodeServiceConfigPair {
        DistributedNodeServiceConfigPair {
            local: self.component_config_for_local_service(port),
            remote: self.component_config_for_remote_service(port),
        }
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

fn get_signature_manager_component_config(
    signature_manager_local_config: ReactiveComponentExecutionConfig,
) -> ComponentConfig {
    let mut config = ComponentConfig::disabled();
    config.signature_manager = signature_manager_local_config;
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
    signature_manager_remote_config: ReactiveComponentExecutionConfig,
) -> ComponentConfig {
    let mut config = ComponentConfig::disabled();
    config.consensus_manager = ActiveComponentExecutionConfig::enabled();
    config.batcher = batcher_remote_config;
    config.class_manager = class_manager_remote_config;
    config.l1_gas_price_provider = l1_gas_price_provider_remote_config;
    config.state_sync = state_sync_remote_config;
    config.monitoring_endpoint = ActiveComponentExecutionConfig::enabled();
    config.signature_manager = signature_manager_remote_config;
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
    l1_endpoint_monitor_local_config: ReactiveComponentExecutionConfig,
    state_sync_remote_config: ReactiveComponentExecutionConfig,
    batcher_remote_config: ReactiveComponentExecutionConfig,
) -> ComponentConfig {
    let mut config = ComponentConfig::disabled();

    config.l1_gas_price_provider = l1_gas_price_provider_local_config;
    config.l1_gas_price_scraper = ActiveComponentExecutionConfig::enabled();
    config.l1_provider = l1_provider_local_config;
    config.l1_scraper = ActiveComponentExecutionConfig::enabled();
    config.l1_endpoint_monitor = l1_endpoint_monitor_local_config;
    config.state_sync = state_sync_remote_config;
    config.monitoring_endpoint = ActiveComponentExecutionConfig::enabled();
    config.batcher = batcher_remote_config;
    config
}
