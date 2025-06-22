use std::fmt::Display;
use std::iter::once;
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
use crate::k8s::{Controller, ExternalSecret, Ingress, IngressParams, Resources, Toleration};

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Service {
    #[serde(rename = "name")]
    service_name: ServiceName,
    // TODO(Tsabary): change config path to PathBuf type.
    controller: Controller,
    config_paths: Vec<String>,
    ingress: Option<Ingress>,
    autoscale: bool,
    replicas: usize,
    storage: Option<usize>,
    toleration: Option<Toleration>,
    resources: Resources,
    external_secret: Option<ExternalSecret>,
    #[serde(skip_serializing)]
    environment: Environment,
    anti_affinity: bool,
}

// TODO(Tsabary): remove clippy::too_many_arguments
impl Service {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        service_name: ServiceName,
        external_secret: Option<ExternalSecret>,
        config_filenames: Vec<String>,
        ingress_params: IngressParams,
        // TODO(Tsabary): consider if including the environment is necessary.
        environment: Environment,
    ) -> Self {
        // Configs are loaded by order such that a config may override previous ones.
        // We first list the base config, and then follow with the overrides.
        let config_paths = config_filenames
            .iter()
            .cloned()
            .chain(once(service_name.get_config_file_path()))
            .collect();

        let controller = service_name.get_controller();
        let autoscale = service_name.get_autoscale();
        let toleration = service_name.get_toleration(&environment);
        let ingress = service_name.get_ingress(&environment, ingress_params);
        let storage = service_name.get_storage(&environment);
        let resources = service_name.get_resources(&environment);
        let replicas = service_name.get_replicas(&environment);
        let anti_affinity = service_name.get_anti_affinity(&environment);
        Self {
            service_name,
            config_paths,
            controller,
            ingress,
            autoscale,
            replicas,
            storage,
            toleration,
            resources,
            external_secret,
            environment,
            anti_affinity,
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
        ingress_params: IngressParams,
    ) -> Service {
        Service::new(
            Into::<ServiceName>::into(*self),
            external_secret.clone(),
            additional_config_filenames,
            ingress_params.clone(),
            environment.clone(),
        )
    }

    fn as_inner(&self) -> &dyn ServiceNameInner {
        match self {
            ServiceName::ConsolidatedNode(inner) => inner,
            ServiceName::HybridNode(inner) => inner,
            ServiceName::DistributedNode(inner) => inner,
        }
    }

    pub fn get_controller(&self) -> Controller {
        self.as_inner().get_controller()
    }

    pub fn get_autoscale(&self) -> bool {
        self.as_inner().get_autoscale()
    }

    pub fn get_toleration(&self, environment: &Environment) -> Option<Toleration> {
        self.as_inner().get_toleration(environment)
    }

    pub fn get_ingress(
        &self,
        environment: &Environment,
        ingress_params: IngressParams,
    ) -> Option<Ingress> {
        self.as_inner().get_ingress(environment, ingress_params)
    }

    pub fn get_storage(&self, environment: &Environment) -> Option<usize> {
        self.as_inner().get_storage(environment)
    }

    pub fn get_resources(&self, environment: &Environment) -> Resources {
        self.as_inner().get_resources(environment)
    }

    pub fn get_replicas(&self, environment: &Environment) -> usize {
        self.as_inner().get_replicas(environment)
    }

    pub fn get_anti_affinity(&self, environment: &Environment) -> bool {
        // TODO(Tsabary): implement anti-affinity logic.
        self.as_inner().get_anti_affinity(environment)
    }

    // Kubernetes service name as defined by CDK8s.
    pub fn k8s_service_name(&self) -> String {
        self.as_inner().k8s_service_name()
    }
}

pub(crate) trait ServiceNameInner: Display {
    fn get_controller(&self) -> Controller;

    fn get_autoscale(&self) -> bool;

    fn get_toleration(&self, environment: &Environment) -> Option<Toleration>;

    fn get_ingress(
        &self,
        environment: &Environment,
        ingress_params: IngressParams,
    ) -> Option<Ingress>;

    fn get_storage(&self, environment: &Environment) -> Option<usize>;

    fn get_resources(&self, environment: &Environment) -> Resources;

    fn get_replicas(&self, environment: &Environment) -> usize;

    fn get_anti_affinity(&self, environment: &Environment) -> bool;

    // Kubernetes service name as defined by CDK8s.
    fn k8s_service_name(&self) -> String {
        let formatted_service_name = self.to_string().replace('_', "");
        format!("sequencer-{}-service", formatted_service_name)
    }
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
        ports: Option<Vec<u16>>,
    ) -> IndexMap<ServiceName, ComponentConfig> {
        match self {
            // TODO(Tsabary): avoid this code duplication.
            Self::ConsolidatedNode => ConsolidatedNodeServiceName::get_component_configs(ports),
            Self::HybridNode => HybridNodeServiceName::get_component_configs(ports),
            Self::DistributedNode => DistributedNodeServiceName::get_component_configs(ports),
        }
    }
}

pub trait GetComponentConfigs {
    // TODO(Tsabary): replace IndexMap with regular HashMap. Currently using IndexMap as the
    // integration test relies on indices rather than service names.
    fn get_component_configs(ports: Option<Vec<u16>>) -> IndexMap<ServiceName, ComponentConfig>;
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
