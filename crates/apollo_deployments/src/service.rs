use std::fmt::Display;
use std::iter::once;
use std::path::{Path, PathBuf};

use apollo_config::dumping::SerializeConfig;
use apollo_infra_utils::dumping::serialize_to_file;
#[cfg(test)]
use apollo_infra_utils::dumping::serialize_to_file_test;
use apollo_node::config::component_config::ComponentConfig;
use apollo_node::config::config_utils::config_to_preset;
use indexmap::IndexMap;
use serde::{Serialize, Serializer};
use serde_json::json;
use strum::{Display, EnumVariantNames, IntoEnumIterator};
use strum_macros::{EnumDiscriminants, EnumIter, IntoStaticStr};

#[cfg(test)]
use crate::deployment::FIX_BINARY_NAME;
use crate::deployment::{
    build_service_namespace_domain_address,
    ComponentConfigsSerializationWrapper,
};
use crate::deployment_definitions::{Environment, CONFIG_BASE_DIR};
use crate::deployments::consolidated::ConsolidatedNodeServiceName;
use crate::deployments::distributed::DistributedNodeServiceName;
use crate::deployments::hybrid::HybridNodeServiceName;
use crate::k8s::{
    Controller,
    ExternalSecret,
    Ingress,
    IngressParams,
    K8sServiceConfig,
    K8sServiceConfigParams,
    Resources,
    Toleration,
};

const SERVICES_DIR_NAME: &str = "services/";

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Service {
    #[serde(rename = "name")]
    node_service: NodeService,
    // TODO(Tsabary): change config path to PathBuf type.
    controller: Controller,
    config_paths: Vec<String>,
    ingress: Option<Ingress>,
    k8s_service_config: Option<K8sServiceConfig>,
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

impl Service {
    pub fn new(
        node_service: NodeService,
        external_secret: Option<ExternalSecret>,
        config_filenames: Vec<String>,
        ingress_params: IngressParams,
        k8s_service_config_params: Option<K8sServiceConfigParams>,
        environment: Environment,
    ) -> Self {
        // Configs are loaded by order such that a config may override previous ones.
        // We first list the base config, and then follow with the overrides, and finally, the
        // service config file.

        // TODO(Tsabary): the deployment override file can be in a higher directory.
        // TODO(Tsabary): delete redundant directories in the path.
        // TODO(Tsabary): reduce visibility of relevant functions and consts.

        let service_file_path = node_service.get_service_file_path();

        let config_paths = config_filenames
            .iter()
            .cloned()
            .chain(once(service_file_path))
            .map(|p| {
                // Strip the parent dir prefix.
                Path::new(&p)
                    .strip_prefix(CONFIG_BASE_DIR)
                    .map(|stripped| stripped.to_string_lossy().into_owned())
                    .expect("Failed to strip mutual prefix")
            })
            .collect();

        let controller = node_service.get_controller();
        let autoscale = node_service.get_autoscale();
        let toleration = node_service.get_toleration(&environment);
        let ingress = node_service.get_ingress(&environment, ingress_params);
        let k8s_service_config = node_service.get_k8s_service_config(k8s_service_config_params);
        let storage = node_service.get_storage(&environment);
        let resources = node_service.get_resources(&environment);
        let replicas = node_service.get_replicas(&environment);
        let anti_affinity = node_service.get_anti_affinity(&environment);
        Self {
            node_service,
            config_paths,
            controller,
            ingress,
            k8s_service_config,
            autoscale,
            replicas,
            storage,
            toleration,
            resources,
            external_secret,
            // TODO(Tsabary): consider removing `environment` from the `Service` struct.
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
    name(NodeType),
    derive(IntoStaticStr, EnumIter, EnumVariantNames, Serialize, Display),
    strum(serialize_all = "snake_case")
)]
pub enum NodeService {
    ConsolidatedNode(ConsolidatedNodeServiceName),
    HybridNode(HybridNodeServiceName),
    DistributedNode(DistributedNodeServiceName),
}

impl NodeService {
    fn get_config_file_path(&self) -> String {
        let mut name = self.as_inner().to_string();
        name.push_str(".json");
        name
    }

    pub fn create_service(
        &self,
        environment: &Environment,
        external_secret: &Option<ExternalSecret>,
        config_filenames: Vec<String>,
        ingress_params: IngressParams,
        k8s_service_config_params: Option<K8sServiceConfigParams>,
    ) -> Service {
        Service::new(
            Into::<NodeService>::into(*self),
            external_secret.clone(),
            config_filenames,
            ingress_params.clone(),
            k8s_service_config_params,
            environment.clone(),
        )
    }

    fn as_inner(&self) -> &dyn ServiceNameInner {
        match self {
            NodeService::ConsolidatedNode(inner) => inner,
            NodeService::HybridNode(inner) => inner,
            NodeService::DistributedNode(inner) => inner,
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

    pub fn get_k8s_service_config(
        &self,
        k8s_service_config_params: Option<K8sServiceConfigParams>,
    ) -> Option<K8sServiceConfig> {
        self.as_inner().get_k8s_service_config(k8s_service_config_params)
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

    pub fn get_service_file_path(&self) -> String {
        PathBuf::from(CONFIG_BASE_DIR)
            .join(SERVICES_DIR_NAME)
            .join(NodeType::from(self).get_folder_name())
            .join(self.get_config_file_path())
            .to_string_lossy()
            .to_string()
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

    fn get_k8s_service_config(
        &self,
        k8s_service_config_params: Option<K8sServiceConfigParams>,
    ) -> Option<K8sServiceConfig> {
        if self.has_p2p_interface() {
            if let Some(K8sServiceConfigParams { namespace, domain, p2p_communication_type }) =
                k8s_service_config_params
            {
                let service_namespace_domain = build_service_namespace_domain_address(
                    &self.k8s_service_name(),
                    &namespace,
                    &domain,
                );
                return Some(K8sServiceConfig::new(
                    Some(service_namespace_domain),
                    p2p_communication_type,
                ));
            }
        }
        None
    }

    fn has_p2p_interface(&self) -> bool;

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

impl NodeType {
    pub fn get_folder_name(&self) -> &'static str {
        match self {
            Self::ConsolidatedNode => "consolidated/",
            Self::HybridNode => "hybrid/",
            Self::DistributedNode => "distributed/",
        }
    }

    pub fn add_path_suffix(&self, path: PathBuf, instance_name: &str) -> PathBuf {
        let node_type_dir = path.join(self.get_folder_name());
        let deployment_with_instance = node_type_dir.join(instance_name);

        let s = deployment_with_instance.to_string_lossy();
        let modified = if s.ends_with('/') { s.into_owned() } else { format!("{}/", s) };
        modified.into()
    }

    pub fn all_service_names(&self) -> Vec<NodeService> {
        match self {
            // TODO(Tsabary): find a way to avoid this code duplication.
            Self::ConsolidatedNode => {
                ConsolidatedNodeServiceName::iter().map(NodeService::ConsolidatedNode).collect()
            }
            Self::HybridNode => {
                HybridNodeServiceName::iter().map(NodeService::HybridNode).collect()
            }
            Self::DistributedNode => {
                DistributedNodeServiceName::iter().map(NodeService::DistributedNode).collect()
            }
        }
    }

    pub fn get_component_configs(
        &self,
        ports: Option<Vec<u16>>,
    ) -> IndexMap<NodeService, ComponentConfig> {
        match self {
            // TODO(Tsabary): avoid this code duplication.
            Self::ConsolidatedNode => ConsolidatedNodeServiceName::get_component_configs(ports),
            Self::HybridNode => HybridNodeServiceName::get_component_configs(ports),
            Self::DistributedNode => DistributedNodeServiceName::get_component_configs(ports),
        }
    }

    fn dump_component_configs_with<SerdeFn>(&self, ports: Option<Vec<u16>>, writer: SerdeFn)
    where
        SerdeFn: Fn(&serde_json::Value, &str),
    {
        let component_configs = self.get_component_configs(ports);
        for (node_service, config) in component_configs {
            let wrapper = ComponentConfigsSerializationWrapper::from(config);
            let flattened = config_to_preset(&json!(wrapper.dump()));
            let file_path = node_service.get_service_file_path();
            writer(&flattened, &file_path);
        }
    }

    pub fn dump_service_component_configs(&self, ports: Option<Vec<u16>>) {
        self.dump_component_configs_with(ports, |map, path| {
            serialize_to_file(map, path);
        });
    }

    #[cfg(test)]
    pub fn test_dump_service_component_configs(&self, ports: Option<Vec<u16>>) {
        self.dump_component_configs_with(ports, |map, path| {
            serialize_to_file_test(map, path, FIX_BINARY_NAME);
        });
    }
}

pub trait GetComponentConfigs {
    // TODO(Tsabary): replace IndexMap with regular HashMap. Currently using IndexMap as the
    // integration test relies on indices rather than service names.
    fn get_component_configs(ports: Option<Vec<u16>>) -> IndexMap<NodeService, ComponentConfig>;
}

impl Serialize for NodeService {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Serialize only the inner value.
        match self {
            NodeService::ConsolidatedNode(inner) => inner.serialize(serializer),
            NodeService::HybridNode(inner) => inner.serialize(serializer),
            NodeService::DistributedNode(inner) => inner.serialize(serializer),
        }
    }
}
