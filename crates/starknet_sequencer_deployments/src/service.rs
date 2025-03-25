use std::fmt::Display;

use apollo_sequencer_node::config::component_config::ComponentConfig;
use indexmap::IndexMap;
use serde::{Serialize, Serializer};
use strum::{Display, EnumVariantNames, IntoEnumIterator};
use strum_macros::{EnumDiscriminants, EnumIter, IntoStaticStr};

use crate::deployments::consolidated::ConsolidatedNodeServiceName;
use crate::deployments::distributed::DistributedNodeServiceName;

const DEPLOYMENT_CONFIG_BASE_DIR_PATH: &str = "config/sequencer/presets";
// TODO(Tsabary): need to distinguish between test and production configs in dir structure.
const APPLICATION_CONFIG_DIR_NAME: &str = "application_configs";

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
    DistributedNode(DistributedNodeServiceName),
}

impl ServiceName {
    pub fn get_config_file_path(&self) -> String {
        let mut name = self.as_inner().to_string();
        name.push_str(".json");
        name
    }

    fn as_inner(&self) -> &dyn ServiceNameInner {
        match self {
            ServiceName::ConsolidatedNode(inner) => inner,
            ServiceName::DistributedNode(inner) => inner,
        }
    }
}

pub(crate) trait ServiceNameInner: Display {}

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

pub trait GetComponentConfigs {
    // TODO(Tsabary): replace IndexMap with regular HashMap. Currently using IndexMap as the
    // integration test relies on indices rather than service names.
    fn get_component_configs(base_port: Option<u16>) -> IndexMap<ServiceName, ComponentConfig>;
}

pub trait IntoService {
    fn create_service(&self) -> Service;
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
