use std::net::{IpAddr, Ipv4Addr};
#[cfg(test)]
use std::path::Path;

use serde::{Serialize, Serializer};
use starknet_api::core::ChainId;
use strum_macros::AsRefStr;

use crate::config::component_execution_config::ReactiveComponentExecutionConfig;

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
