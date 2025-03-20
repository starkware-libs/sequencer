use std::net::{IpAddr, Ipv4Addr};
#[cfg(test)]
use std::path::Path;

use indexmap::IndexMap;
use serde::{Serialize, Serializer};
use starknet_api::core::ChainId;
use starknet_sequencer_node::config::component_config::ComponentConfig;
use starknet_sequencer_node::config::component_execution_config::{
    ActiveComponentExecutionConfig,
    ReactiveComponentExecutionConfig,
};
use strum::{Display, EnumVariantNames, IntoEnumIterator};
use strum_macros::{AsRefStr, EnumDiscriminants, EnumIter, IntoStaticStr};

const BASE_PORT: u16 = 55000; // TODO(Tsabary): arbitrary port, need to resolve.
const DEPLOYMENT_IMAGE: &str = "ghcr.io/starkware-libs/sequencer/sequencer:dev";
const DEPLOYMENT_CONFIG_BASE_DIR_PATH: &str = "config/sequencer/presets/";
// TODO(Tsabary): need to distinguish between test and production configs in dir structure.
const APPLICATION_CONFIG_DIR_NAME: &str = "application_configs";

pub struct DeploymentAndPreset {
    pub deployment: Deployment,
    pub dump_file_path: &'static str,
}

impl DeploymentAndPreset {
    pub fn new(deployment: Deployment, dump_file_path: &'static str) -> Self {
        Self { deployment, dump_file_path }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Deployment {
    chain_id: ChainId,
    image: &'static str,
    application_config_subdir: String,
    services: Vec<Service>,
}

impl Deployment {
    pub fn new(chain_id: ChainId, deployment_name: DeploymentName) -> Self {
        let service_names = deployment_name.all_service_names();
        let services =
            service_names.iter().map(|service_name| service_name.create_service()).collect();
        Self {
            chain_id,
            image: DEPLOYMENT_IMAGE,
            application_config_subdir: deployment_name.get_path(),
            services,
        }
    }

    #[cfg(test)]
    pub fn assert_application_configs_exist(&self) {
        // TODO(Tsabary): avoid cloning here.
        for service in self.services.clone() {
            // Concatenate paths.
            let subdir_path = Path::new(&self.application_config_subdir);
            let full_path = subdir_path.join(service.config_path);
            // Assert existence.
            assert!(full_path.exists(), "File does not exist: {:?}", full_path);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Service {
    name: ServiceName,
    // TODO(Tsabary): change config path to PathBuf type.
    config_path: String,
    ingress: bool,
    autoscale: bool,
    replicas: usize,
    storage: Option<usize>,
}

impl Service {
    pub fn new(
        name: ServiceName,
        ingress: bool,
        autoscale: bool,
        replicas: usize,
        storage: Option<usize>,
    ) -> Self {
        let config_path = name.get_config_file_path();
        Self { name, config_path, ingress, autoscale, replicas, storage }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, EnumDiscriminants)]
#[strum_discriminants(
    name(DeploymentName),
    derive(IntoStaticStr, EnumIter, EnumVariantNames, Serialize, Display),
    strum(serialize_all = "snake_case")
)]
pub enum ServiceName {
    ConsolidatedNode(ConsolidatedNodeServiceName),
    DistributedNode(DistributedNodeServiceName),
}

impl ServiceName {
    pub fn get_config_file_path(&self) -> String {
        // TODO(Tsabary): find a way to avoid this code duplication.
        let mut name = match self {
            Self::ConsolidatedNode(inner) => inner.to_string(),
            Self::DistributedNode(inner) => inner.to_string(),
        };
        name.push_str(".json");
        name
    }
}

// Implement conversion from `DistributedNodeServiceName` to `ServiceName`
impl From<DistributedNodeServiceName> for ServiceName {
    fn from(service: DistributedNodeServiceName) -> Self {
        ServiceName::DistributedNode(service)
    }
}

// Implement conversion from `ConsolidatedNodeServiceName` to `ServiceName`
impl From<ConsolidatedNodeServiceName> for ServiceName {
    fn from(service: ConsolidatedNodeServiceName) -> Self {
        ServiceName::ConsolidatedNode(service)
    }
}

impl IntoService for ServiceName {
    fn create_service(&self) -> Service {
        // TODO(Tsabary): find a way to avoid this code duplication.
        match self {
            Self::ConsolidatedNode(inner) => inner.create_service(),
            Self::DistributedNode(inner) => inner.create_service(),
        }
    }
}

impl DeploymentName {
    pub fn all_service_names(&self) -> Vec<ServiceName> {
        match self {
            // TODO(Tsabary): find a way to avoid this code duplication.
            Self::ConsolidatedNode => {
                ConsolidatedNodeServiceName::iter().map(ServiceName::ConsolidatedNode).collect()
            }
            Self::DistributedNode => {
                DistributedNodeServiceName::iter().map(ServiceName::DistributedNode).collect()
            }
        }
    }

    pub fn get_path(&self) -> String {
        format!("{}/{}/{}/", DEPLOYMENT_CONFIG_BASE_DIR_PATH, self, APPLICATION_CONFIG_DIR_NAME)
    }

    pub fn get_component_configs(
        &self,
        base_port: Option<u16>,
    ) -> IndexMap<ServiceName, ComponentConfig> {
        match self {
            // TODO(Tsabary): avoid this code duplication.
            Self::ConsolidatedNode => ConsolidatedNodeServiceName::get_component_configs(base_port),
            Self::DistributedNode => DistributedNodeServiceName::get_component_configs(base_port),
        }
    }
}

#[cfg(any(feature = "testing", test))]
pub fn set_urls_to_localhost(component_configs: &mut [ComponentConfig]) {
    for component_config in component_configs.iter_mut() {
        component_config.set_urls_to_localhost();
    }
}

// TODO(Tsabary): each deployment should be in its own module.

pub trait GetComponentConfigs {
    // TODO(Tsabary): replace IndexMap with regular HashMap. Currently using IndexMap as the
    // integration test relies on indices rather than service names.
    fn get_component_configs(base_port: Option<u16>) -> IndexMap<ServiceName, ComponentConfig>;
}

impl GetComponentConfigs for ConsolidatedNodeServiceName {
    fn get_component_configs(_base_port: Option<u16>) -> IndexMap<ServiceName, ComponentConfig> {
        let mut component_config_map = IndexMap::new();
        component_config_map.insert(
            ServiceName::ConsolidatedNode(ConsolidatedNodeServiceName::Node),
            get_consolidated_config(),
        );
        component_config_map
    }
}

impl GetComponentConfigs for DistributedNodeServiceName {
    fn get_component_configs(base_port: Option<u16>) -> IndexMap<ServiceName, ComponentConfig> {
        let mut component_config_map = IndexMap::<ServiceName, ComponentConfig>::new();

        let batcher = DistributedNodeServiceName::Batcher.component_config_pair(base_port);
        let class_manager =
            DistributedNodeServiceName::ClassManager.component_config_pair(base_port);
        let gateway = DistributedNodeServiceName::Gateway.component_config_pair(base_port);
        let l1_provider = DistributedNodeServiceName::L1Provider.component_config_pair(base_port);
        let mempool = DistributedNodeServiceName::Mempool.component_config_pair(base_port);
        let sierra_compiler =
            DistributedNodeServiceName::SierraCompiler.component_config_pair(base_port);
        let state_sync = DistributedNodeServiceName::StateSync.component_config_pair(base_port);

        for inner_service_name in DistributedNodeServiceName::iter() {
            let component_config = match inner_service_name {
                DistributedNodeServiceName::Batcher => get_batcher_config(
                    batcher.local(),
                    class_manager.remote(),
                    l1_provider.remote(),
                    mempool.remote(),
                ),
                DistributedNodeServiceName::ClassManager => {
                    get_class_manager_config(class_manager.local(), sierra_compiler.remote())
                }
                DistributedNodeServiceName::ConsensusManager => get_consensus_manager_config(
                    batcher.remote(),
                    class_manager.remote(),
                    state_sync.remote(),
                ),
                DistributedNodeServiceName::HttpServer => get_http_server_config(gateway.remote()),
                DistributedNodeServiceName::Gateway => get_gateway_config(
                    gateway.local(),
                    class_manager.remote(),
                    mempool.remote(),
                    state_sync.remote(),
                ),
                DistributedNodeServiceName::L1Provider => {
                    get_l1_provider_config(l1_provider.local(), state_sync.remote())
                }
                DistributedNodeServiceName::Mempool => {
                    get_mempool_config(mempool.local(), class_manager.remote(), gateway.remote())
                }
                DistributedNodeServiceName::SierraCompiler => {
                    get_sierra_compiler_config(sierra_compiler.local())
                }
                DistributedNodeServiceName::StateSync => {
                    get_state_sync_config(state_sync.local(), class_manager.remote())
                }
            };
            let service_name = inner_service_name.into();
            component_config_map.insert(service_name, component_config);
        }
        component_config_map
    }
}

pub trait IntoService {
    fn create_service(&self) -> Service;
}

impl IntoService for ConsolidatedNodeServiceName {
    fn create_service(&self) -> Service {
        match self {
            ConsolidatedNodeServiceName::Node => {
                Service::new(Into::<ServiceName>::into(*self), false, false, 1, Some(32))
            }
        }
    }
}

impl Serialize for ServiceName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // TODO(Tsabary): find a way to avoid this code duplication.
        match self {
            ServiceName::ConsolidatedNode(inner) => inner.serialize(serializer), /* Serialize only the inner value */
            ServiceName::DistributedNode(inner) => inner.serialize(serializer), /* Serialize only the inner value */
        }
    }
}

#[derive(Clone, Copy, Debug, Display, PartialEq, Eq, Hash, Serialize, AsRefStr, EnumIter)]
#[strum(serialize_all = "snake_case")]
pub enum ConsolidatedNodeServiceName {
    Node,
}

#[repr(u16)]
#[derive(Clone, Copy, Debug, Display, PartialEq, Eq, Hash, Serialize, AsRefStr, EnumIter)]
#[strum(serialize_all = "snake_case")]
pub enum DistributedNodeServiceName {
    Batcher,
    ClassManager,
    ConsensusManager,
    HttpServer,
    Gateway,
    L1Provider,
    Mempool,
    SierraCompiler,
    StateSync,
}

// TODO(Tsabary): per each service, update all values.
impl IntoService for DistributedNodeServiceName {
    fn create_service(&self) -> Service {
        match self {
            DistributedNodeServiceName::Batcher => {
                Service::new(Into::<ServiceName>::into(*self), false, false, 1, Some(32))
            }
            DistributedNodeServiceName::ClassManager => {
                Service::new(Into::<ServiceName>::into(*self), false, false, 1, Some(32))
            }
            DistributedNodeServiceName::ConsensusManager => {
                Service::new(Into::<ServiceName>::into(*self), false, false, 1, Some(32))
            }
            DistributedNodeServiceName::HttpServer => {
                Service::new(Into::<ServiceName>::into(*self), false, false, 1, Some(32))
            }
            DistributedNodeServiceName::Gateway => {
                Service::new(Into::<ServiceName>::into(*self), false, false, 1, Some(32))
            }
            DistributedNodeServiceName::L1Provider => {
                Service::new(Into::<ServiceName>::into(*self), false, false, 1, Some(32))
            }
            DistributedNodeServiceName::Mempool => {
                Service::new(Into::<ServiceName>::into(*self), false, false, 1, Some(32))
            }
            DistributedNodeServiceName::SierraCompiler => {
                Service::new(Into::<ServiceName>::into(*self), false, false, 1, Some(32))
            }
            DistributedNodeServiceName::StateSync => {
                Service::new(Into::<ServiceName>::into(*self), false, false, 1, Some(32))
            }
        }
    }
}

impl DistributedNodeServiceName {
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

    fn component_config_pair(&self, base_port: Option<u16>) -> DistributedNodeServiceConfigPair {
        DistributedNodeServiceConfigPair {
            local: self.component_config_for_local_service(base_port),
            remote: self.component_config_for_remote_service(base_port),
        }
    }

    /// Url for the service.
    fn url(&self) -> String {
        format!("http://{}/", self.as_ref())
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

// TODO(Tsabary): temporarily bundling all distributed node config functions here. This module will
// need to be split into multiple per-deployment modules, each with its relevant functions.

fn get_batcher_config(
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

fn get_class_manager_config(
    class_manager_local_config: ReactiveComponentExecutionConfig,
    sierra_compiler_remote_config: ReactiveComponentExecutionConfig,
) -> ComponentConfig {
    let mut config = ComponentConfig::disabled();
    config.class_manager = class_manager_local_config;
    config.sierra_compiler = sierra_compiler_remote_config;
    config.monitoring_endpoint = ActiveComponentExecutionConfig::enabled();
    config
}

fn get_gateway_config(
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

fn get_mempool_config(
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

fn get_sierra_compiler_config(
    sierra_compiler_local_config: ReactiveComponentExecutionConfig,
) -> ComponentConfig {
    let mut config = ComponentConfig::disabled();
    config.sierra_compiler = sierra_compiler_local_config;
    config.monitoring_endpoint = ActiveComponentExecutionConfig::enabled();
    config
}

fn get_state_sync_config(
    state_sync_local_config: ReactiveComponentExecutionConfig,
    class_manager_remote_config: ReactiveComponentExecutionConfig,
) -> ComponentConfig {
    let mut config = ComponentConfig::disabled();
    config.state_sync = state_sync_local_config;
    config.class_manager = class_manager_remote_config;
    config.monitoring_endpoint = ActiveComponentExecutionConfig::enabled();
    config
}

fn get_consensus_manager_config(
    batcher_remote_config: ReactiveComponentExecutionConfig,
    class_manager_remote_config: ReactiveComponentExecutionConfig,
    state_sync_remote_config: ReactiveComponentExecutionConfig,
) -> ComponentConfig {
    let mut config = ComponentConfig::disabled();
    config.consensus_manager = ActiveComponentExecutionConfig::enabled();
    config.batcher = batcher_remote_config;
    config.class_manager = class_manager_remote_config;
    config.state_sync = state_sync_remote_config;
    config.monitoring_endpoint = ActiveComponentExecutionConfig::enabled();
    config
}

fn get_http_server_config(
    gateway_remote_config: ReactiveComponentExecutionConfig,
) -> ComponentConfig {
    let mut config = ComponentConfig::disabled();
    config.http_server = ActiveComponentExecutionConfig::enabled();
    config.gateway = gateway_remote_config;
    config.monitoring_endpoint = ActiveComponentExecutionConfig::enabled();
    config
}

fn get_l1_provider_config(
    l1_provider_local_config: ReactiveComponentExecutionConfig,
    state_sync_remote_config: ReactiveComponentExecutionConfig,
) -> ComponentConfig {
    let mut config = ComponentConfig::disabled();
    config.l1_provider = l1_provider_local_config;
    config.l1_scraper = ActiveComponentExecutionConfig::enabled();
    config.state_sync = state_sync_remote_config;
    config.monitoring_endpoint = ActiveComponentExecutionConfig::enabled();
    config
}
// TODO(Tsabary): rename get_X_config fns to get_X_component_config.

// TODO(Tsabary): functions for the consolidated node deployment, need move to a different module.
fn get_consolidated_config() -> ComponentConfig {
    ComponentConfig::default()
}
