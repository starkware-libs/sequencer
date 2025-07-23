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

use crate::deployment_definitions::{Environment, ServicePort};
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
use crate::utils::determine_port_numbers;

pub const DISTRIBUTED_NODE_REQUIRED_PORTS_NUM: usize = 14;

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
        let mut service_ports: BTreeMap<ServicePort, u16> = BTreeMap::new();
        match ports {
            Some(ports) => {
                let determined_ports = determine_port_numbers(
                    Some(ports),
                    DISTRIBUTED_NODE_REQUIRED_PORTS_NUM,
                    BASE_PORT,
                );
                for (service_port, port) in ServicePort::iter().zip(determined_ports) {
                    service_ports.insert(service_port, port);
                }
            }
            None => {
                // Extract the service ports for all inner services of the distributed node.
                for inner_service_name in DistributedNodeServiceName::iter() {
                    let inner_service_port = inner_service_name.get_service_port_mapping();
                    service_ports.extend(inner_service_port);
                }
            }
        };

        let batcher = DistributedNodeServiceName::Batcher
            .component_config_pair(service_ports[&ServicePort::Batcher]);
        let class_manager = DistributedNodeServiceName::ClassManager
            .component_config_pair(service_ports[&ServicePort::ClassManager]);
        let gateway = DistributedNodeServiceName::Gateway
            .component_config_pair(service_ports[&ServicePort::Gateway]);
        let l1_gas_price_provider = DistributedNodeServiceName::L1
            .component_config_pair(service_ports[&ServicePort::L1GasPriceProvider]);
        let l1_provider = DistributedNodeServiceName::L1
            .component_config_pair(service_ports[&ServicePort::L1Provider]);
        let l1_endpoint_monitor = DistributedNodeServiceName::L1
            .component_config_pair(service_ports[&ServicePort::L1EndpointMonitor]);
        let mempool = DistributedNodeServiceName::Mempool
            .component_config_pair(service_ports[&ServicePort::Mempool]);
        let sierra_compiler = DistributedNodeServiceName::SierraCompiler
            .component_config_pair(service_ports[&ServicePort::SierraCompiler]);
        let state_sync = DistributedNodeServiceName::StateSync
            .component_config_pair(service_ports[&ServicePort::StateSync]);
        let signature_manager = DistributedNodeServiceName::SignatureManager
            .component_config_pair(service_ports[&ServicePort::SignatureManager]);

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
                DistributedNodeServiceName::SignatureManager => {
                    get_signature_manager_component_config(signature_manager.local())
                }
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
            Environment::Testing => None,
            Environment::SepoliaIntegration
            | Environment::SepoliaTestnet
            | Environment::UpgradeTest
            | Environment::TestingEnvThree
            | Environment::StressTest => match self {
                DistributedNodeServiceName::Batcher => Some(Toleration::ApolloCoreService),
                DistributedNodeServiceName::ClassManager => Some(Toleration::ApolloGeneralService),
                DistributedNodeServiceName::ConsensusManager => Some(Toleration::ApolloCoreService),
                DistributedNodeServiceName::HttpServer => Some(Toleration::ApolloGeneralService),
                DistributedNodeServiceName::Gateway => Some(Toleration::ApolloGeneralService),
                DistributedNodeServiceName::L1 => Some(Toleration::ApolloGeneralService),
                DistributedNodeServiceName::Mempool => Some(Toleration::ApolloCoreService),
                DistributedNodeServiceName::SierraCompiler => {
                    Some(Toleration::ApolloGeneralService)
                }
                DistributedNodeServiceName::SignatureManager => {
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
            Environment::Testing => None,
            Environment::SepoliaIntegration
            | Environment::SepoliaTestnet
            | Environment::UpgradeTest
            | Environment::TestingEnvThree
            | Environment::StressTest => match self {
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
            _ => unimplemented!(),
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
            Environment::Testing => false,
            Environment::SepoliaIntegration
            | Environment::SepoliaTestnet
            | Environment::UpgradeTest
            | Environment::TestingEnvThree
            | Environment::StressTest => match self {
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
            _ => unimplemented!(),
        }
    }

    fn get_service_ports(&self) -> BTreeSet<ServicePort> {
        let mut service_ports = BTreeSet::new();

        match self {
            DistributedNodeServiceName::Batcher => {
                for service_port in ServicePort::iter() {
                    match service_port {
                        ServicePort::MonitoringEndpoint => {
                            service_ports.insert(ServicePort::MonitoringEndpoint);
                        }
                        ServicePort::Batcher => {
                            service_ports.insert(ServicePort::Batcher);
                        }
                        ServicePort::HttpServer
                        | ServicePort::ClassManager
                        | ServicePort::ConsensusManager
                        | ServicePort::Gateway
                        | ServicePort::L1EndpointMonitor
                        | ServicePort::L1GasPriceProvider
                        | ServicePort::L1Provider
                        | ServicePort::Mempool
                        | ServicePort::MempoolP2p
                        | ServicePort::SignatureManager
                        | ServicePort::StateSync
                        | ServicePort::SierraCompiler => {}
                    }
                }
            }
            DistributedNodeServiceName::ClassManager => {
                for service_port in ServicePort::iter() {
                    match service_port {
                        ServicePort::MonitoringEndpoint => {
                            service_ports.insert(ServicePort::MonitoringEndpoint);
                        }
                        ServicePort::ClassManager => {
                            service_ports.insert(ServicePort::ClassManager);
                        }
                        ServicePort::Batcher
                        | ServicePort::ConsensusManager
                        | ServicePort::Gateway
                        | ServicePort::L1EndpointMonitor
                        | ServicePort::L1GasPriceProvider
                        | ServicePort::L1Provider
                        | ServicePort::Mempool
                        | ServicePort::MempoolP2p
                        | ServicePort::SierraCompiler
                        | ServicePort::StateSync
                        | ServicePort::HttpServer
                        | ServicePort::SignatureManager => {}
                    }
                }
            }
            DistributedNodeServiceName::ConsensusManager => {
                for service_port in ServicePort::iter() {
                    match service_port {
                        ServicePort::MonitoringEndpoint => {
                            service_ports.insert(ServicePort::MonitoringEndpoint);
                        }
                        ServicePort::ConsensusManager => {
                            service_ports.insert(ServicePort::ClassManager);
                        }
                        ServicePort::Batcher
                        | ServicePort::ClassManager
                        | ServicePort::Gateway
                        | ServicePort::HttpServer
                        | ServicePort::L1EndpointMonitor
                        | ServicePort::L1GasPriceProvider
                        | ServicePort::L1Provider
                        | ServicePort::Mempool
                        | ServicePort::MempoolP2p
                        | ServicePort::SignatureManager
                        | ServicePort::StateSync
                        | ServicePort::SierraCompiler => {}
                    }
                }
            }
            DistributedNodeServiceName::HttpServer => {
                for service_port in ServicePort::iter() {
                    match service_port {
                        ServicePort::MonitoringEndpoint => {
                            service_ports.insert(ServicePort::MonitoringEndpoint);
                        }
                        ServicePort::HttpServer => {
                            service_ports.insert(ServicePort::HttpServer);
                        }
                        ServicePort::Batcher
                        | ServicePort::ClassManager
                        | ServicePort::ConsensusManager
                        | ServicePort::Gateway
                        | ServicePort::L1EndpointMonitor
                        | ServicePort::L1GasPriceProvider
                        | ServicePort::L1Provider
                        | ServicePort::Mempool
                        | ServicePort::MempoolP2p
                        | ServicePort::SignatureManager
                        | ServicePort::StateSync
                        | ServicePort::SierraCompiler => {}
                    }
                }
            }
            DistributedNodeServiceName::Gateway => {
                for service_port in ServicePort::iter() {
                    match service_port {
                        ServicePort::MonitoringEndpoint => {
                            service_ports.insert(ServicePort::MonitoringEndpoint);
                        }
                        ServicePort::Gateway => {
                            service_ports.insert(ServicePort::Gateway);
                        }
                        ServicePort::Batcher
                        | ServicePort::ClassManager
                        | ServicePort::ConsensusManager
                        | ServicePort::L1EndpointMonitor
                        | ServicePort::L1GasPriceProvider
                        | ServicePort::L1Provider
                        | ServicePort::Mempool
                        | ServicePort::MempoolP2p
                        | ServicePort::SignatureManager
                        | ServicePort::StateSync
                        | ServicePort::HttpServer
                        | ServicePort::SierraCompiler => {}
                    }
                }
            }
            DistributedNodeServiceName::L1 => {
                for service_port in ServicePort::iter() {
                    match service_port {
                        ServicePort::MonitoringEndpoint => {
                            service_ports.insert(ServicePort::MonitoringEndpoint);
                        }
                        ServicePort::L1EndpointMonitor => {
                            service_ports.insert(ServicePort::L1EndpointMonitor);
                        }
                        ServicePort::L1GasPriceProvider => {
                            service_ports.insert(ServicePort::L1GasPriceProvider);
                        }
                        ServicePort::L1Provider => {
                            service_ports.insert(ServicePort::L1Provider);
                        }
                        ServicePort::Batcher
                        | ServicePort::ClassManager
                        | ServicePort::ConsensusManager
                        | ServicePort::Gateway
                        | ServicePort::HttpServer
                        | ServicePort::Mempool
                        | ServicePort::MempoolP2p
                        | ServicePort::SignatureManager
                        | ServicePort::StateSync
                        | ServicePort::SierraCompiler => {}
                    }
                }
            }
            DistributedNodeServiceName::Mempool => {
                for service_port in ServicePort::iter() {
                    match service_port {
                        ServicePort::MonitoringEndpoint => {
                            service_ports.insert(ServicePort::MonitoringEndpoint);
                        }
                        ServicePort::Mempool => {
                            service_ports.insert(ServicePort::Mempool);
                        }
                        ServicePort::Batcher
                        | ServicePort::ClassManager
                        | ServicePort::ConsensusManager
                        | ServicePort::Gateway
                        | ServicePort::L1EndpointMonitor
                        | ServicePort::L1GasPriceProvider
                        | ServicePort::L1Provider
                        | ServicePort::MempoolP2p
                        | ServicePort::SignatureManager
                        | ServicePort::StateSync
                        | ServicePort::HttpServer
                        | ServicePort::SierraCompiler => {}
                    }
                }
            }
            DistributedNodeServiceName::SierraCompiler => {
                for service_port in ServicePort::iter() {
                    match service_port {
                        ServicePort::MonitoringEndpoint => {
                            service_ports.insert(ServicePort::MonitoringEndpoint);
                        }
                        ServicePort::SierraCompiler => {
                            service_ports.insert(ServicePort::SierraCompiler);
                        }
                        ServicePort::Batcher
                        | ServicePort::HttpServer
                        | ServicePort::ClassManager
                        | ServicePort::ConsensusManager
                        | ServicePort::L1EndpointMonitor
                        | ServicePort::L1GasPriceProvider
                        | ServicePort::L1Provider
                        | ServicePort::StateSync
                        | ServicePort::Mempool
                        | ServicePort::Gateway
                        | ServicePort::MempoolP2p
                        | ServicePort::SignatureManager => {}
                    }
                }
            }
            DistributedNodeServiceName::SignatureManager => {
                for service_port in ServicePort::iter() {
                    match service_port {
                        ServicePort::MonitoringEndpoint => {
                            service_ports.insert(ServicePort::MonitoringEndpoint);
                        }
                        ServicePort::SignatureManager => {
                            service_ports.insert(ServicePort::SignatureManager);
                        }
                        ServicePort::ConsensusManager => {
                            service_ports.insert(ServicePort::ConsensusManager);
                        }
                        ServicePort::Batcher
                        | ServicePort::HttpServer
                        | ServicePort::ClassManager
                        | ServicePort::L1EndpointMonitor
                        | ServicePort::L1GasPriceProvider
                        | ServicePort::L1Provider
                        | ServicePort::StateSync
                        | ServicePort::Mempool
                        | ServicePort::Gateway
                        | ServicePort::MempoolP2p
                        | ServicePort::SierraCompiler => {}
                    }
                }
            }
            DistributedNodeServiceName::StateSync => {
                for service_port in ServicePort::iter() {
                    match service_port {
                        ServicePort::MonitoringEndpoint => {
                            service_ports.insert(ServicePort::MonitoringEndpoint);
                        }
                        ServicePort::StateSync => {
                            service_ports.insert(ServicePort::StateSync);
                        }
                        ServicePort::Batcher
                        | ServicePort::ClassManager
                        | ServicePort::ConsensusManager
                        | ServicePort::Gateway
                        | ServicePort::HttpServer
                        | ServicePort::L1EndpointMonitor
                        | ServicePort::L1GasPriceProvider
                        | ServicePort::L1Provider
                        | ServicePort::Mempool
                        | ServicePort::MempoolP2p
                        | ServicePort::SignatureManager
                        | ServicePort::SierraCompiler => {}
                    }
                }
            }
        };

        service_ports
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
                base.remote_client_config.idle_connections =
                    IDLE_CONNECTIONS_FOR_AUTOSCALED_SERVICES
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
