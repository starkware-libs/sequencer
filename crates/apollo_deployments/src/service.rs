use std::collections::BTreeMap;
use std::fmt::Display;
use std::iter::once;
use std::path::PathBuf;

use apollo_config::dumping::SerializeConfig;
use apollo_infra_utils::dumping::serialize_to_file;
#[cfg(test)]
use apollo_infra_utils::dumping::serialize_to_file_test;
use apollo_node::config::component_config::ComponentConfig;
use apollo_node::config::config_utils::config_to_preset;
use indexmap::IndexMap;
use serde::ser::SerializeSeq;
use serde::{Serialize, Serializer};
use serde_json::json;
use strum::{Display, EnumVariantNames, IntoEnumIterator};
use strum_macros::{EnumDiscriminants, EnumIter, IntoStaticStr};

use crate::deployment::{
    build_service_namespace_domain_address,
    ComponentConfigsSerializationWrapper,
};
use crate::deployment_definitions::{Environment, ServicePort, CONFIG_BASE_DIR};
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
#[cfg(test)]
use crate::test_utils::FIX_BINARY_NAME;

const SERVICES_DIR_NAME: &str = "services/";

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Service {
    #[serde(rename = "name")]
    node_service: NodeService,
    // TODO(Tsabary): change config path to PathBuf type.
    controller: Controller,
    #[serde(serialize_with = "serialize_vec_strip_prefix")]
    config_paths: Vec<String>,
    ingress: Option<Ingress>,
    k8s_service_config: Option<K8sServiceConfig>,
    autoscale: bool,
    replicas: usize,
    storage: Option<usize>,
    toleration: Option<Toleration>,
    resources: Resources,
    external_secret: Option<ExternalSecret>,
    anti_affinity: bool,
    ports: BTreeMap<ServicePort, u16>,
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

        let config_paths =
            config_filenames.iter().cloned().chain(once(service_file_path)).collect();

        let controller = node_service.get_controller();
        let autoscale = node_service.get_autoscale();
        let toleration = node_service.get_toleration(&environment);
        let ingress = node_service.get_ingress(&environment, ingress_params);
        let k8s_service_config = node_service.get_k8s_service_config(k8s_service_config_params);
        let storage = node_service.get_storage(&environment);
        let resources = node_service.get_resources(&environment);
        let replicas = node_service.get_replicas(&environment);
        let anti_affinity = node_service.get_anti_affinity(&environment);
        let ports = node_service.get_ports();
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
            anti_affinity,
            ports,
        }
    }

    pub fn get_service_config_paths(&self) -> Vec<String> {
        self.config_paths.clone()
    }
}

fn serialize_vec_strip_prefix<S>(vec: &Vec<String>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut seq = serializer.serialize_seq(Some(vec.len()))?;

    for s in vec {
        if let Some(stripped) = s.strip_prefix(CONFIG_BASE_DIR) {
            seq.serialize_element(stripped)?;
        } else {
            return Err(serde::ser::Error::custom(format!(
                "Expected all items to start with '{}', got '{}'",
                CONFIG_BASE_DIR, s
            )));
        }
    }

    seq.end()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, EnumDiscriminants)]
#[strum_discriminants(
    name(NodeType),
    derive(IntoStaticStr, EnumIter, EnumVariantNames, Serialize, Display),
    strum(serialize_all = "snake_case")
)]
pub enum NodeService {
    Consolidated(ConsolidatedNodeServiceName),
    Hybrid(HybridNodeServiceName),
    Distributed(DistributedNodeServiceName),
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
            NodeService::Consolidated(inner) => inner,
            NodeService::Hybrid(inner) => inner,
            NodeService::Distributed(inner) => inner,
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

    pub fn get_ports(&self) -> BTreeMap<ServicePort, u16> {
        self.as_inner().get_ports()
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

    fn get_ports(&self) -> BTreeMap<ServicePort, u16>;

    // Kubernetes service name as defined by CDK8s.
    fn k8s_service_name(&self) -> String {
        let formatted_service_name = self.to_string().replace('_', "");
        format!("sequencer-{}-service", formatted_service_name)
    }
}

impl NodeType {
    fn get_folder_name(&self) -> String {
        self.to_string()
    }

    pub fn all_service_names(&self) -> Vec<NodeService> {
        match self {
            // TODO(Tsabary): find a way to avoid this code duplication.
            Self::Consolidated => {
                ConsolidatedNodeServiceName::iter().map(NodeService::Consolidated).collect()
            }
            Self::Hybrid => HybridNodeServiceName::iter().map(NodeService::Hybrid).collect(),
            Self::Distributed => {
                DistributedNodeServiceName::iter().map(NodeService::Distributed).collect()
            }
        }
    }

    pub fn get_component_configs(
        &self,
        ports: Option<Vec<u16>>,
    ) -> IndexMap<NodeService, ComponentConfig> {
        match self {
            // TODO(Tsabary): avoid this code duplication.
            Self::Consolidated => ConsolidatedNodeServiceName::get_component_configs(ports),
            Self::Hybrid => HybridNodeServiceName::get_component_configs(ports),
            Self::Distributed => DistributedNodeServiceName::get_component_configs(ports),
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
            NodeService::Consolidated(inner) => inner.serialize(serializer),
            NodeService::Hybrid(inner) => inner.serialize(serializer),
            NodeService::Distributed(inner) => inner.serialize(serializer),
        }
    }
}
