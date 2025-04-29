use std::fmt::Display;
use std::path::PathBuf;

use apollo_node::config::component_config::ComponentConfig;
use indexmap::IndexMap;
use serde::{Serialize, Serializer};
use strum::{Display, EnumVariantNames, IntoEnumIterator};
use strum_macros::{EnumDiscriminants, EnumIter, IntoStaticStr};

use crate::deployment_definitions::Environment;
use crate::deployments::consolidated::ConsolidatedNodeServiceName;
use crate::deployments::distributed::DistributedNodeServiceName;
use crate::deployments::hybrid::HybridNodeServiceName;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Service {
    name: ServiceName,
    // TODO(Tsabary): change config path to PathBuf type.
    controller: Controller,
    config_paths: Vec<String>,
    ingress: Option<Ingress>,
    autoscale: bool,
    replicas: usize,
    storage: Option<usize>,
    toleration: Option<String>,
    resources: Resources,
    external_secret: Option<ExternalSecret>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum Controller {
    Deployment,
    StatefulSet,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Ingress {
    domain: String,
    internal: bool,
    rules: Vec<IngressRule>,
    alternative_names: Vec<String>,
}

impl Ingress {
    pub fn new(
        domain: String,
        internal: bool,
        rules: Vec<IngressRule>,
        alternative_names: Vec<String>,
    ) -> Self {
        Self { domain, internal, rules, alternative_names }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct IngressRule {
    path: String,
    port: u16,
    backend: Option<String>,
}

impl IngressRule {
    pub fn new(path: String, port: u16, backend: Option<String>) -> Self {
        Self { path, port, backend }
    }
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
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: ServiceName,
        controller: Controller,
        ingress: Option<Ingress>,
        autoscale: bool,
        replicas: usize,
        storage: Option<usize>,
        toleration: Option<String>,
        resources: Resources,
        external_secret: Option<ExternalSecret>,
        mut additional_config_filenames: Vec<String>,
    ) -> Self {
        // Configs are loaded by order such that a config may override previous ones.
        // We first list the base config, and then follow with the overrides.
        // TODO(Tsabary): the service override is currently engrained in the base config, need to
        // resolve that.
        let mut config_paths: Vec<String> = vec![name.get_config_file_path()];
        config_paths.append(&mut additional_config_filenames);

        Self {
            name,
            config_paths,
            controller,
            ingress,
            autoscale,
            replicas,
            storage,
            toleration,
            resources,
            external_secret,
        }
    }

    pub fn get_config_paths(&self) -> Vec<String> {
        self.config_paths.clone()
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

    pub fn create_service(
        &self,
        environment: &Environment,
        external_secret: &Option<ExternalSecret>,
        additional_config_filenames: Vec<String>,
        domain: String,
        ingress_alternative_names: Option<Vec<String>>,
    ) -> Service {
        self.as_inner().create_service(
            environment,
            external_secret,
            additional_config_filenames,
            domain,
            ingress_alternative_names,
        )
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
    fn create_service(
        &self,
        environment: &Environment,
        external_secret: &Option<ExternalSecret>,
        additional_config_filenames: Vec<String>,
        domain: String,
        ingress_alternative_names: Option<Vec<String>>,
    ) -> Service;
}

impl DeploymentName {
    pub fn add_path_suffix(&self, path: PathBuf, instance_name: &str) -> PathBuf {
        let deployment_name_dir = match self {
            // TODO(Tsabary): find a way to avoid this code duplication.
            // Trailing backslash needed to mitigate deployment test issues.
            Self::ConsolidatedNode => path.join("consolidated/"),
            Self::HybridNode => path.join("hybrid/"),
            Self::DistributedNode => path.join("distributed/"),
        };
        println!("Deployment name dir: {:?}", deployment_name_dir);
        let deployment_with_instance = deployment_name_dir.join(instance_name);
        println!("Deployment with instance: {:?}", deployment_with_instance);

        let s = deployment_with_instance.to_string_lossy();
        let modified = if s.ends_with('/') { s.into_owned() } else { format!("{}/", s) };
        modified.into()
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
        environment: &Environment,
    ) -> IndexMap<ServiceName, ComponentConfig> {
        match self {
            // TODO(Tsabary): avoid this code duplication.
            Self::ConsolidatedNode => {
                ConsolidatedNodeServiceName::get_component_configs(base_port, environment)
            }
            Self::HybridNode => {
                HybridNodeServiceName::get_component_configs(base_port, environment)
            }
            Self::DistributedNode => {
                DistributedNodeServiceName::get_component_configs(base_port, environment)
            }
        }
    }
}

pub trait GetComponentConfigs {
    // TODO(Tsabary): replace IndexMap with regular HashMap. Currently using IndexMap as the
    // integration test relies on indices rather than service names.
    fn get_component_configs(
        base_port: Option<u16>,
        environment: &Environment,
    ) -> IndexMap<ServiceName, ComponentConfig>;
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
