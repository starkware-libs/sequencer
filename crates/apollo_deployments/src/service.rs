use std::fmt::Display;
use std::path::PathBuf;

use apollo_node::config::component_config::ComponentConfig;
use indexmap::IndexMap;
use serde::{Serialize, Serializer};
use strum::{Display, EnumVariantNames, IntoEnumIterator};
use strum_macros::{EnumDiscriminants, EnumIter, IntoStaticStr};

use crate::deployments::consolidated::ConsolidatedNodeServiceName;
use crate::deployments::distributed::DistributedNodeServiceName;
use crate::deployments::hybrid::HybridNodeServiceName;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Service {
    name: ServiceName,
    // TODO(Tsabary): change config path to PathBuf type.
    config_path: String,
    ingress: bool,
    autoscale: bool,
    replicas: usize,
    storage: Option<usize>,
    resources: Resources,
    external_secret: Option<ExternalSecret>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ExternalSecret {
    gcsm_key: &'static str,
}

impl ExternalSecret {
    pub fn new(gcsm_key: &'static str) -> Self {
        Self { gcsm_key }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Resource {
    cpu: usize,
    memory: usize,
}

impl Resource {
    pub fn new(cpu: usize, memory: usize) -> Self {
        Self { cpu, memory }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Resources {
    requests: Resource,
    limits: Resource,
}

impl Resources {
    pub fn new(requests: Resource, limits: Resource) -> Self {
        Self { requests, limits }
    }
}

impl Service {
    pub fn new(
        name: ServiceName,
        ingress: bool,
        autoscale: bool,
        replicas: usize,
        storage: Option<usize>,
        resources: Resources,
        external_secret: Option<ExternalSecret>,
    ) -> Self {
        Self {
            name,
            config_path: name.get_config_file_path(),
            ingress,
            autoscale,
            replicas,
            storage,
            resources,
            external_secret,
        }
    }

    pub fn get_config_path(&self) -> String {
        self.config_path.clone()
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
    HybridNode(HybridNodeServiceName),
    DistributedNode(DistributedNodeServiceName),
}

impl ServiceName {
    pub fn get_config_file_path(&self) -> String {
        let mut name = self.as_inner().to_string();
        name.push_str(".json");
        name
    }

    pub fn create_service(&self) -> Service {
        self.as_inner().create_service()
    }

    fn as_inner(&self) -> &dyn ServiceNameInner {
        match self {
            ServiceName::ConsolidatedNode(inner) => inner,
            ServiceName::HybridNode(inner) => inner,
            ServiceName::DistributedNode(inner) => inner,
        }
    }
}

pub(crate) trait ServiceNameInner: Display {
    fn create_service(&self) -> Service;
}

impl DeploymentName {
    pub fn add_path_suffix(&self, path: PathBuf) -> PathBuf {
        match self {
            // TODO(Tsabary): find a way to avoid this code duplication.
            // Trailing backslash needed to mitigate deployment test issues.
            Self::ConsolidatedNode => path.join("consolidated/"),
            Self::HybridNode => path.join("hybrid/"),
            Self::DistributedNode => path.join("distributed/"),
        }
    }

    pub fn all_service_names(&self) -> Vec<ServiceName> {
        match self {
            // TODO(Tsabary): find a way to avoid this code duplication.
            Self::ConsolidatedNode => {
                ConsolidatedNodeServiceName::iter().map(ServiceName::ConsolidatedNode).collect()
            }
            Self::HybridNode => {
                HybridNodeServiceName::iter().map(ServiceName::HybridNode).collect()
            }
            Self::DistributedNode => {
                DistributedNodeServiceName::iter().map(ServiceName::DistributedNode).collect()
            }
        }
    }

    pub fn get_component_configs(
        &self,
        base_port: Option<u16>,
    ) -> IndexMap<ServiceName, ComponentConfig> {
        match self {
            // TODO(Tsabary): avoid this code duplication.
            Self::ConsolidatedNode => ConsolidatedNodeServiceName::get_component_configs(base_port),
            Self::HybridNode => HybridNodeServiceName::get_component_configs(base_port),
            Self::DistributedNode => DistributedNodeServiceName::get_component_configs(base_port),
        }
    }
}

pub trait GetComponentConfigs {
    // TODO(Tsabary): replace IndexMap with regular HashMap. Currently using IndexMap as the
    // integration test relies on indices rather than service names.
    fn get_component_configs(base_port: Option<u16>) -> IndexMap<ServiceName, ComponentConfig>;
}

impl Serialize for ServiceName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Serialize only the inner value.
        match self {
            ServiceName::ConsolidatedNode(inner) => inner.serialize(serializer),
            ServiceName::HybridNode(inner) => inner.serialize(serializer),
            ServiceName::DistributedNode(inner) => inner.serialize(serializer),
        }
    }
}
