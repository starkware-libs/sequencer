use std::collections::{BTreeMap, BTreeSet};
use std::net::{IpAddr, Ipv4Addr};

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
use crate::deployment_definitions::{CloudK8sEnvironment, Environment, ServicePort};
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

pub const HYBRID_NODE_REQUIRED_PORTS_NUM: usize = 14;
pub(crate) const INSTANCE_NAME_FORMAT: Template = Template("hybrid_{}");

const BASE_PORT: u16 = 55000; // TODO(Tsabary): arbitrary port, need to resolve.
const CORE_STORAGE: usize = 1000;
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

        let mut service_ports: BTreeMap<ServicePort, u16> = BTreeMap::new();
        match ports {
            Some(ports) => {
                let determined_ports =
                    determine_port_numbers(Some(ports), HYBRID_NODE_REQUIRED_PORTS_NUM, BASE_PORT);
                for (service_port, port) in ServicePort::iter().zip(determined_ports) {
                    service_ports.insert(service_port, port);
                }
            }
            None => {
                // Extract the service ports for all inner services of the hybrid node.
                for inner_service_name in HybridNodeServiceName::iter() {
                    let inner_service_port = inner_service_name.get_service_port_mapping();
                    service_ports.extend(inner_service_port);
                }
            }
        };

        let batcher =
            HybridNodeServiceName::Core.component_config_pair(service_ports[&ServicePort::Batcher]);
        let class_manager = HybridNodeServiceName::Core
            .component_config_pair(service_ports[&ServicePort::ClassManager]);
        let gateway = HybridNodeServiceName::Gateway
            .component_config_pair(service_ports[&ServicePort::Gateway]);
        let l1_gas_price_provider = HybridNodeServiceName::Core
            .component_config_pair(service_ports[&ServicePort::L1GasPriceProvider]);
        let l1_provider = HybridNodeServiceName::Core
            .component_config_pair(service_ports[&ServicePort::L1Provider]);
        let mempool = HybridNodeServiceName::Mempool
            .component_config_pair(service_ports[&ServicePort::Mempool]);
        let sierra_compiler = HybridNodeServiceName::SierraCompiler
            .component_config_pair(service_ports[&ServicePort::SierraCompiler]);
        let state_sync = HybridNodeServiceName::Core
            .component_config_pair(service_ports[&ServicePort::StateSync]);
        let signature_manager = HybridNodeServiceName::Core
            .component_config_pair(service_ports[&ServicePort::SignatureManager]);

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
            Environment::LocalK8s => None,
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
                        ServicePort::MonitoringEndpoint => {
                            service_ports.insert(ServicePort::MonitoringEndpoint);
                        }
                        ServicePort::Batcher => {
                            service_ports.insert(ServicePort::Batcher);
                        }
                        ServicePort::ClassManager => {
                            service_ports.insert(ServicePort::ClassManager);
                        }
                        // TODO(Nadin): Move these to the L1 service once it's merged to main.
                        ServicePort::L1EndpointMonitor => {
                            service_ports.insert(ServicePort::L1EndpointMonitor);
                        }
                        ServicePort::L1GasPriceProvider => {
                            service_ports.insert(ServicePort::L1GasPriceProvider);
                        }
                        ServicePort::L1Provider => {
                            service_ports.insert(ServicePort::L1Provider);
                        }
                        ServicePort::StateSync => {
                            service_ports.insert(ServicePort::StateSync);
                        }
                        ServicePort::ConsensusManager => {
                            service_ports.insert(ServicePort::ConsensusManager);
                        }
                        ServicePort::SignatureManager => {
                            service_ports.insert(ServicePort::SignatureManager);
                        }
                        ServicePort::HttpServer
                        | ServicePort::Gateway
                        | ServicePort::Mempool
                        | ServicePort::MempoolP2p
                        | ServicePort::SierraCompiler => {}
                    }
                }
            }
            HybridNodeServiceName::HttpServer => {
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
                        | ServicePort::L1EndpointMonitor
                        | ServicePort::L1GasPriceProvider
                        | ServicePort::L1Provider
                        | ServicePort::StateSync
                        | ServicePort::Mempool
                        | ServicePort::Gateway
                        | ServicePort::MempoolP2p
                        | ServicePort::SignatureManager
                        | ServicePort::SierraCompiler => {}
                    }
                }
            }
            HybridNodeServiceName::Gateway => {
                for service_port in ServicePort::iter() {
                    match service_port {
                        ServicePort::MonitoringEndpoint => {
                            service_ports.insert(ServicePort::MonitoringEndpoint);
                        }
                        ServicePort::Gateway => {
                            service_ports.insert(ServicePort::Gateway);
                        }
                        ServicePort::HttpServer
                        | ServicePort::Batcher
                        | ServicePort::ClassManager
                        | ServicePort::ConsensusManager
                        | ServicePort::L1EndpointMonitor
                        | ServicePort::L1GasPriceProvider
                        | ServicePort::L1Provider
                        | ServicePort::StateSync
                        | ServicePort::Mempool
                        | ServicePort::MempoolP2p
                        | ServicePort::SignatureManager
                        | ServicePort::SierraCompiler => {}
                    }
                }
            }
            HybridNodeServiceName::L1 => {
                for service_port in ServicePort::iter() {
                    match service_port {
                        ServicePort::MonitoringEndpoint => {
                            service_ports.insert(ServicePort::MonitoringEndpoint);
                        }
                        ServicePort::HttpServer
                        | ServicePort::Batcher
                        | ServicePort::ClassManager
                        | ServicePort::ConsensusManager
                        | ServicePort::L1EndpointMonitor
                        | ServicePort::L1GasPriceProvider
                        | ServicePort::L1Provider
                        | ServicePort::StateSync
                        | ServicePort::Mempool
                        | ServicePort::Gateway
                        | ServicePort::MempoolP2p
                        | ServicePort::SignatureManager
                        | ServicePort::SierraCompiler => {
                            // TODO(Nadin): should define the ports for these services (if needed).
                        }
                    }
                }
            }
            HybridNodeServiceName::Mempool => {
                for service_port in ServicePort::iter() {
                    match service_port {
                        ServicePort::MonitoringEndpoint => {
                            service_ports.insert(ServicePort::MonitoringEndpoint);
                        }
                        ServicePort::Mempool => {
                            service_ports.insert(ServicePort::Mempool);
                        }
                        ServicePort::HttpServer
                        | ServicePort::Batcher
                        | ServicePort::ClassManager
                        | ServicePort::ConsensusManager
                        | ServicePort::L1EndpointMonitor
                        | ServicePort::L1GasPriceProvider
                        | ServicePort::L1Provider
                        | ServicePort::StateSync
                        | ServicePort::Gateway
                        | ServicePort::MempoolP2p
                        | ServicePort::SignatureManager
                        | ServicePort::SierraCompiler => {}
                    }
                }
            }
            HybridNodeServiceName::SierraCompiler => {
                for service_port in ServicePort::iter() {
                    match service_port {
                        ServicePort::MonitoringEndpoint => {
                            service_ports.insert(ServicePort::MonitoringEndpoint);
                        }
                        ServicePort::SierraCompiler => {
                            service_ports.insert(ServicePort::SierraCompiler);
                        }
                        ServicePort::HttpServer
                        | ServicePort::Batcher
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
        };
        service_ports
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

// TODO(Tsaabry): unify these into inner structs.
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
