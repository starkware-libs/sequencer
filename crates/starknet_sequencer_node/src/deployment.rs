#[cfg(test)]
use std::path::Path;

use serde::{Serialize, Serializer};
use starknet_api::core::ChainId;

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
#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum DistributedNodeServiceName {
    Mempool,
    Gateway,
    Batcher,
}
