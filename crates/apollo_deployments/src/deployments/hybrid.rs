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
    Core,    // Comprises the batcher, class manager, consensus manager, and state sync.
    Gateway, // Comprises the gateway and http server
    L1,      // Comprises the various l1 components.
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
                for inner_service_name in HybridNodeServiceName::iter() {
                    let inner_service_port = inner_service_name.get_infra_service_port_mapping();
                    service_ports.extend(inner_service_port);
                }
            }
        };

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
            .component_config_pair(service_ports[&InfraServicePort::SierraCompiler]);
        let signature_manager = HybridNodeServiceName::Core
            .component_config_pair(service_ports[&InfraServicePort::SignatureManager]);
        let state_sync = HybridNodeServiceName::Core
            .component_config_pair(service_ports[&InfraServicePort::StateSync]);

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
                    signature_manager.local(),
                ),
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
            HybridNodeServiceName::Gateway => Controller::Deployment,
            HybridNodeServiceName::L1 => Controller::Deployment,
            HybridNodeServiceName::Mempool => Controller::Deployment,
            HybridNodeServiceName::SierraCompiler => Controller::Deployment,
        }
    }

    fn get_scale_policy(&self) -> ScalePolicy {
        match self {
            HybridNodeServiceName::Core
            | HybridNodeServiceName::L1
            | HybridNodeServiceName::Mempool => ScalePolicy::StaticallyScaled,

            HybridNodeServiceName::Gateway | HybridNodeServiceName::SierraCompiler => {
                ScalePolicy::AutoScaled
            }
        }
    }

    fn get_retries(&self) -> usize {
        match self {
            Self::Core | Self::Mempool | Self::Gateway | Self::SierraCompiler => DEFAULT_RETRIES,
            Self::L1 => RETRIES_FOR_L1_SERVICES,
        }
    }

    fn get_toleration(&self, _environment: &Environment) -> Option<Toleration> {
        None
    }

    fn get_ingress(
        &self,
        _environment: &Environment,
        _ingress_params: IngressParams,
    ) -> Option<Ingress> {
        None
    }

    fn has_p2p_interface(&self) -> bool {
        match self {
            HybridNodeServiceName::Core | HybridNodeServiceName::Mempool => true,
            HybridNodeServiceName::Gateway
            | HybridNodeServiceName::L1
            | HybridNodeServiceName::SierraCompiler => false,
        }
    }

    fn get_storage(&self, _environment: &Environment) -> Option<usize> {
        match self {
            HybridNodeServiceName::Core => Some(TEST_CORE_STORAGE),
            HybridNodeServiceName::Gateway
            | HybridNodeServiceName::L1
            | HybridNodeServiceName::Mempool
            | HybridNodeServiceName::SierraCompiler => None,
        }
    }

    fn get_resources(&self, _environment: &Environment) -> Resources {
        Resources::new(Resource::new(1, 2), Resource::new(4, 8))
    }

    fn get_replicas(&self, _environment: &Environment) -> usize {
        1
    }

    fn get_anti_affinity(&self, _environment: &Environment) -> bool {
        false
    }

    fn get_service_ports(&self) -> BTreeSet<ServicePort> {
        let mut service_ports = BTreeSet::new();

        match self {
            HybridNodeServiceName::Core => {
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
            HybridNodeServiceName::Gateway => {
                for service_port in ServicePort::iter() {
                    match service_port {
                        ServicePort::BusinessLogic(bl_port) => match bl_port {
                            BusinessLogicServicePort::HttpServer
                            | BusinessLogicServicePort::MonitoringEndpoint => {
                                service_ports.insert(service_port);
                            }
                            BusinessLogicServicePort::ConsensusP2p
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
            HybridNodeServiceName::Mempool => {
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
            HybridNodeServiceName::SierraCompiler => {
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
            HybridNodeServiceName::Core => {
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
            HybridNodeServiceName::Gateway => {
                for component_config_in_service in ComponentConfigInService::iter() {
                    match component_config_in_service {
                        ComponentConfigInService::ConfigManager
                        | ComponentConfigInService::Gateway
                        | ComponentConfigInService::HttpServer
                        | ComponentConfigInService::General
                        | ComponentConfigInService::MonitoringEndpoint => {
                            components.insert(component_config_in_service);
                        }
                        ComponentConfigInService::BaseLayer
                        | ComponentConfigInService::Batcher
                        | ComponentConfigInService::ClassManager
                        | ComponentConfigInService::Consensus
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
            HybridNodeServiceName::L1 => {
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
            HybridNodeServiceName::Mempool => {
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
            HybridNodeServiceName::SierraCompiler => {
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
            HybridNodeServiceName::Core => UpdateStrategy::RollingUpdate,
            HybridNodeServiceName::Gateway => UpdateStrategy::RollingUpdate,
            HybridNodeServiceName::L1 => UpdateStrategy::Recreate,
            HybridNodeServiceName::Mempool => UpdateStrategy::Recreate,
            HybridNodeServiceName::SierraCompiler => UpdateStrategy::RollingUpdate,
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
    config.http_server = ActiveComponentExecutionConfig::enabled();
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
