use std::fmt::Display;
use std::iter::once;
use std::path::PathBuf;

use apollo_node::config::component_config::ComponentConfig;
use indexmap::IndexMap;
use serde::{Serialize, Serializer};
use strum::{Display, EnumVariantNames, IntoEnumIterator};
use strum_macros::{EnumDiscriminants, EnumIter, IntoStaticStr};

use crate::deployment::P2PCommunicationType;
use crate::deployment_definitions::Environment;
use crate::deployments::consolidated::ConsolidatedNodeServiceName;
use crate::deployments::distributed::DistributedNodeServiceName;
use crate::deployments::hybrid::HybridNodeServiceName;

// Controls whether external P2P communication is enabled.
const ENABLE_EXTERNAL_P2P_COMMUNICATION: bool = false;

const INGRESS_ROUTE: &str = "/gateway";
const INGRESS_PORT: u16 = 8080;

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

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum Controller {
    Deployment,
    StatefulSet,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum K8SServiceType {
    ClusterIp,
    LoadBalancer,
    NodePort,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct K8sServiceConfig {
    #[serde(rename = "type")]
    k8s_service_type: K8SServiceType,
    external_dns_name: Option<String>,
    internal: bool,
}

impl K8sServiceConfig {
    pub fn new(
        external_dns_name: Option<String>,
        p2p_communication_type: P2PCommunicationType,
    ) -> Self {
        Self {
            k8s_service_type: p2p_communication_type.get_k8s_service_type(),
            external_dns_name,
            internal: ENABLE_EXTERNAL_P2P_COMMUNICATION,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Ingress {
    #[serde(flatten)]
    ingress_params: IngressParams,
    internal: bool,
    rules: Vec<IngressRule>,
}

impl Ingress {
    pub fn new(ingress_params: IngressParams, internal: bool, rules: Vec<IngressRule>) -> Self {
        Self { ingress_params, internal, rules }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct IngressParams {
    domain: String,
    #[serde(serialize_with = "serialize_none_as_empty_vec")]
    alternative_names: Option<Vec<String>>,
}

fn serialize_none_as_empty_vec<S, T>(
    value: &Option<Vec<T>>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    T: Serialize,
{
    match value {
        Some(v) => serializer.serialize_some(v),
        None => serializer.serialize_some(&Vec::<T>::new()),
    }
}

impl IngressParams {
    pub fn new(domain: String, alternative_names: Option<Vec<String>>) -> Self {
        Self { domain, alternative_names }
    }
}

pub(crate) fn get_ingress(ingress_params: IngressParams, internal: bool) -> Option<Ingress> {
    Some(Ingress::new(
        ingress_params,
        internal,
        vec![IngressRule::new(String::from(INGRESS_ROUTE), INGRESS_PORT, None)],
    ))
}

pub(crate) fn get_environment_ingress_internal(environment: &Environment) -> bool {
    match environment {
        Environment::Testing => true,
        Environment::SepoliaIntegration
        | Environment::TestingEnvTwo
        | Environment::TestingEnvThree
        | Environment::StressTest => false,
        _ => unimplemented!(),
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
    gcsm_key: String,
}

impl ExternalSecret {
    pub fn new(gcsm_key: impl ToString) -> Self {
        Self { gcsm_key: gcsm_key.to_string() }
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

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum Toleration {
    ApolloCoreService,
    ApolloCoreServiceC2D16,
    ApolloCoreServiceC2D32,
    ApolloCoreServiceC2D56,
    ApolloGeneralService,
}
