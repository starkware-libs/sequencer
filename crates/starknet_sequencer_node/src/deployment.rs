use std::net::{IpAddr, Ipv4Addr};
#[cfg(test)]
use std::path::Path;

use serde::{Serialize, Serializer};
use starknet_api::core::ChainId;
use strum_macros::AsRefStr;

use crate::config::component_config::ComponentConfig;
use crate::config::component_execution_config::{
    ActiveComponentExecutionConfig,
    ReactiveComponentExecutionConfig,
};

const BASE_PORT: u16 = 55000; // TODO(Tsabary): arbitrary port, need to resolve.

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Deployment<'a> {
    chain_id: ChainId,
    image: &'a str,
    application_config_subdir: &'a str,
    services: &'a [Service],
}

impl<'a> Deployment<'a> {
    pub const fn new(
        chain_id: ChainId,
        image: &'a str,
        application_config_subdir: &'a str,
        services: &'a [Service],
    ) -> Self {
        Self { chain_id, image, application_config_subdir, services }
    }

    #[cfg(test)]
    pub fn assert_application_configs_exist(&self) {
        for service in self.services {
            // Concatenate paths.
            let subdir_path = Path::new(self.application_config_subdir);
            let full_path = subdir_path.join(service.config_path);
            // Assert existence.
            assert!(full_path.exists(), "File does not exist: {:?}", full_path);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Service {
    name: ServiceName,
    config_path: &'static str,
    ingress: bool,
    autoscale: bool,
    replicas: usize,
    storage: Option<usize>,
}

impl Service {
    pub const fn new(
        name: ServiceName,
        config_path: &'static str,
        ingress: bool,
        autoscale: bool,
        replicas: usize,
        storage: Option<usize>,
    ) -> Self {
        Self { name, config_path, ingress, autoscale, replicas, storage }
    }
}

#[derive(Clone, Debug, PartialEq)]

pub enum ServiceName {
    ConsolidatedNode,
    DistributedNode(DistributedNodeServiceName),
}

impl Serialize for ServiceName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            ServiceName::ConsolidatedNode => serializer.serialize_str("ConsolidatedNode"),
            ServiceName::DistributedNode(inner) => inner.serialize(serializer), /* Serialize only the inner value */
        }
    }
}

// TODO(Tsabary): sort these.
#[repr(u16)]
#[derive(Clone, Debug, PartialEq, Serialize, AsRefStr)]
pub enum DistributedNodeServiceName {
    Mempool,
    Gateway,
    Batcher,
}

impl DistributedNodeServiceName {
    /// Returns a component execution config for a component that runs locally, and accepts inbound
    /// connections from remote components.
    pub fn component_config_for_local_service(&self) -> ReactiveComponentExecutionConfig {
        ReactiveComponentExecutionConfig::local_with_remote_enabled(
            self.url(),
            self.ip(),
            self.port(),
        )
    }

    /// Returns a component execution config for a component that is accessed remotely.
    pub fn component_config_for_remote_service(&self) -> ReactiveComponentExecutionConfig {
        ReactiveComponentExecutionConfig::remote(self.url(), self.ip(), self.port())
    }

    /// Url for the service.
    fn url(&self) -> String {
        format!("http://{}/", self.as_ref())
    }

    /// Unique port number per service.
    fn port(&self) -> u16 {
        let port_offset = self.get_port_offset();
        BASE_PORT + port_offset
    }

    /// Listening address per service.
    fn ip(&self) -> IpAddr {
        IpAddr::from(Ipv4Addr::UNSPECIFIED)
    }

    // Use the enum discriminant to generate a unique port per service.
    // TODO(Tsabary): consider alternatives that enable removing the linter suppression.
    #[allow(clippy::as_conversions)]
    fn get_port_offset(&self) -> u16 {
        self.clone() as u16
    }
}

/// Component config bundling for services of a distributed node: a config to run a component
/// locally while being accessible to other services, and a suitable config enabling such services
/// the access.
pub struct DistributedNodeServiceConfigPair {
    local: ReactiveComponentExecutionConfig,
    remote: ReactiveComponentExecutionConfig,
}

impl From<DistributedNodeServiceName> for DistributedNodeServiceConfigPair {
    fn from(service_name: DistributedNodeServiceName) -> Self {
        Self {
            local: service_name.component_config_for_local_service(),
            remote: service_name.component_config_for_remote_service(),
        }
    }
}

impl DistributedNodeServiceConfigPair {
    pub fn new(url: String, ip: IpAddr, port: u16) -> Self {
        Self {
            local: ReactiveComponentExecutionConfig::local_with_remote_enabled(
                url.clone(),
                ip,
                port,
            ),
            remote: ReactiveComponentExecutionConfig::remote(url, ip, port),
        }
    }

    pub fn local(&self) -> ReactiveComponentExecutionConfig {
        self.local.clone()
    }

    pub fn remote(&self) -> ReactiveComponentExecutionConfig {
        self.remote.clone()
    }
}

// TODO(Tsabary): temporarily bundling all distributed node config functions here. This module will
// need to be split into multiple per-deployment modules, each with its relevant functions.

pub fn get_batcher_config(
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

pub fn get_class_manager_config(
    class_manager_local_config: ReactiveComponentExecutionConfig,
    sierra_compiler_remote_config: ReactiveComponentExecutionConfig,
) -> ComponentConfig {
    let mut config = ComponentConfig::disabled();
    config.class_manager = class_manager_local_config;
    config.sierra_compiler = sierra_compiler_remote_config;
    config.monitoring_endpoint = ActiveComponentExecutionConfig::enabled();
    config
}

pub fn get_gateway_config(
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

pub fn get_mempool_config(
    mempool_local_config: ReactiveComponentExecutionConfig,
    mempool_p2p_local_config: ReactiveComponentExecutionConfig,
    class_manager_remote_config: ReactiveComponentExecutionConfig,
    gateway_remote_config: ReactiveComponentExecutionConfig,
) -> ComponentConfig {
    let mut config = ComponentConfig::disabled();
    config.mempool = mempool_local_config;
    config.mempool_p2p = mempool_p2p_local_config;
    config.class_manager = class_manager_remote_config;
    config.gateway = gateway_remote_config;
    config.monitoring_endpoint = ActiveComponentExecutionConfig::enabled();
    config
}

pub fn get_sierra_compiler_config(
    sierra_compiler_local_config: ReactiveComponentExecutionConfig,
) -> ComponentConfig {
    let mut config = ComponentConfig::disabled();
    config.sierra_compiler = sierra_compiler_local_config;
    config.monitoring_endpoint = ActiveComponentExecutionConfig::enabled();
    config
}

pub fn get_state_sync_config(
    state_sync_local_config: ReactiveComponentExecutionConfig,
    class_manager_remote_config: ReactiveComponentExecutionConfig,
) -> ComponentConfig {
    let mut config = ComponentConfig::disabled();
    config.state_sync = state_sync_local_config;
    config.class_manager = class_manager_remote_config;
    config.monitoring_endpoint = ActiveComponentExecutionConfig::enabled();
    config
}

pub fn get_consensus_manager_config(
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

pub fn get_http_server_config(
    gateway_remote_config: ReactiveComponentExecutionConfig,
) -> ComponentConfig {
    let mut config = ComponentConfig::disabled();
    config.http_server = ActiveComponentExecutionConfig::enabled();
    config.gateway = gateway_remote_config;
    config.monitoring_endpoint = ActiveComponentExecutionConfig::enabled();
    config
}

pub fn get_l1_provider_config(
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
