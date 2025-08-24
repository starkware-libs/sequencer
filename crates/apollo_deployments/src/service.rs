use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Display;
use std::iter::once;
use std::path::PathBuf;

use apollo_config::dumping::{prepend_sub_config_name, ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam, FIELD_SEPARATOR, IS_NONE_MARK};
use apollo_infra_utils::dumping::serialize_to_file;
#[cfg(test)]
use apollo_infra_utils::dumping::serialize_to_file_test;
use apollo_node::config::component_config::ComponentConfig;
use apollo_node::config::config_utils::{config_to_preset, prune_by_is_none};
use indexmap::IndexMap;
use serde::ser::SerializeSeq;
use serde::{Serialize, Serializer};
use serde_json::json;
use strum::{Display, EnumVariantNames, IntoEnumIterator};
use strum_macros::{EnumDiscriminants, EnumIter, IntoStaticStr};

use crate::deployment::build_service_namespace_domain_address;
use crate::deployment_definitions::{
    ComponentConfigInService,
    Environment,
    InfraServicePort,
    ServicePort,
    CONFIG_BASE_DIR,
};
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
use crate::update_strategy::UpdateStrategy;

const SERVICES_DIR_NAME: &str = "services/";

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Service {
    #[serde(rename = "name")]
    node_service: NodeService,
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
    #[serde(rename = "update_strategy_type")]
    update_strategy: UpdateStrategy,
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

        // TODO(Tsabary): reduce visibility of relevant functions and consts.

        let service_file_path = node_service.get_service_file_path();

        let components_in_service = node_service
            .get_components_in_service()
            .into_iter()
            .flat_map(|c| c.get_component_config_file_paths())
            .collect::<Vec<_>>();
        let config_paths = components_in_service
            .into_iter()
            .chain(config_filenames)
            .chain(once(service_file_path))
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
        let ports = node_service.get_service_port_mapping();
        let update_strategy = node_service.get_update_strategy();
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
            update_strategy,
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
                "Expected all items to start with '{CONFIG_BASE_DIR}', got '{s}'"
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

    fn get_service_file_path(&self) -> String {
        PathBuf::from(CONFIG_BASE_DIR)
            .join(SERVICES_DIR_NAME)
            .join(NodeType::from(self).get_folder_name())
            .join(self.get_config_file_path())
            .to_string_lossy()
            .to_string()
    }

    fn get_components_in_service(&self) -> BTreeSet<ComponentConfigInService> {
        self.as_inner().get_components_in_service()
    }

    pub fn get_service_port_mapping(&self) -> BTreeMap<ServicePort, u16> {
        self.as_inner().get_service_port_mapping()
    }

    pub fn get_update_strategy(&self) -> UpdateStrategy {
        self.as_inner().get_update_strategy()
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

    fn get_service_ports(&self) -> BTreeSet<ServicePort>;

    fn get_service_port_mapping(&self) -> BTreeMap<ServicePort, u16> {
        let mut ports = BTreeMap::new();

        for service_port in self.get_service_ports() {
            let port = service_port.get_port();
            ports.insert(service_port, port);
        }
        ports
    }

    fn get_infra_service_port_mapping(&self) -> BTreeMap<InfraServicePort, u16> {
        let mut ports = BTreeMap::new();

        for service_port in self.get_service_ports() {
            match service_port {
                ServicePort::Infra(service) => {
                    let port = service.get_port();
                    ports.insert(service, port);
                }
                ServicePort::BusinessLogic(_) => {
                    continue;
                }
            }
        }
        ports
    }

    // Kubernetes service name as defined by CDK8s.
    fn k8s_service_name(&self) -> String {
        let formatted_service_name = self.to_string().replace('_', "");
        format!("sequencer-{formatted_service_name}-service")
    }

    fn get_components_in_service(&self) -> BTreeSet<ComponentConfigInService>;

    fn get_update_strategy(&self) -> UpdateStrategy;
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
        for (node_service, component_config) in component_configs {
            let components_in_service = node_service.get_components_in_service();
            let wrapper =
                ComponentConfigsSerializationWrapper::new(component_config, components_in_service);
            let flattened = config_to_preset(&json!(wrapper.dump()));
            let pruned = prune_by_is_none(flattened);
            let file_path = node_service.get_service_file_path();
            writer(&pruned, &file_path);
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

// A helper struct for serializing the components config in the same hierarchy as of its
// serialization as part of the entire config, i.e., by prepending "components.".
#[derive(Clone, Debug, Default, Serialize)]
struct ComponentConfigsSerializationWrapper {
    component_config: ComponentConfig,
    components_in_service: BTreeSet<ComponentConfigInService>,
}

impl ComponentConfigsSerializationWrapper {
    fn new(
        component_config: ComponentConfig,
        components_in_service: BTreeSet<ComponentConfigInService>,
    ) -> Self {
        ComponentConfigsSerializationWrapper { component_config, components_in_service }
    }
}

impl SerializeConfig for ComponentConfigsSerializationWrapper {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut map = prepend_sub_config_name(self.component_config.dump(), "components");
        for component_config_in_service in ComponentConfigInService::iter() {
            if component_config_in_service == ComponentConfigInService::General {
                // General configs are not toggle-able, i.e., no need to add their existence to the
                // service config.
                continue;
            }
            let component_config_names = component_config_in_service.get_component_config_names();
            let is_in_service = self.components_in_service.contains(&component_config_in_service);
            for component_config_name in component_config_names {
                let (param_path, serialized_param) = ser_param(
                    &format!("{component_config_name}{FIELD_SEPARATOR}{IS_NONE_MARK}"),
                    &!is_in_service, // Marking the config as None.
                    "Placeholder description",
                    ParamPrivacyInput::Public,
                );
                map.insert(param_path, serialized_param);
            }
        }
        map
    }
}
