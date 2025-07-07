use std::collections::BTreeMap;
use std::iter::once;
use std::path::PathBuf;

use apollo_config::dumping::{prepend_sub_config_name, SerializeConfig};
use apollo_config::{ParamPath, SerializedParam};
use apollo_node::config::component_config::ComponentConfig;
use apollo_node::config::config_utils::config_to_preset;
use indexmap::IndexMap;
use serde::Serialize;
use serde_json::{json, Value};

use crate::config_override::ConfigOverride;
use crate::deployment_definitions::{Environment, BASE_APP_CONFIG_PATH, CONFIG_BASE_DIR};
use crate::k8s::{ExternalSecret, IngressParams, K8SServiceType, K8sServiceConfigParams};
use crate::service::{NodeService, NodeType, Service};

#[derive(Clone, Debug, Serialize)]
pub struct Deployment {
    application_config_subdir: PathBuf,
    services: Vec<Service>,
    #[serde(skip_serializing)]
    deployment_aux_data: DeploymentAuxData,
}

impl Deployment {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        node_type: NodeType,
        environment: Environment,
        instance_name: &str,
        external_secret: Option<ExternalSecret>,
        config_override: ConfigOverride,
        ingress_params: IngressParams,
        k8s_service_config_params: Option<K8sServiceConfigParams>,
    ) -> Self {
        let node_services = node_type.all_service_names();

        let config_override_files =
            config_override.get_config_file_paths(&environment.env_dir_path(), instance_name);
        let config_filenames: Vec<String> =
            once(BASE_APP_CONFIG_PATH.to_string()).chain(config_override_files).collect();

        let services = node_services
            .iter()
            .map(|node_service| {
                node_service.create_service(
                    &environment,
                    &external_secret,
                    config_filenames.clone(),
                    ingress_params.clone(),
                    k8s_service_config_params.clone(),
                )
            })
            .collect();
        Self {
            application_config_subdir: CONFIG_BASE_DIR.into(),
            services,
            deployment_aux_data: DeploymentAuxData {
                node_type,
                environment,
                instance_name: instance_name.to_string(),
                config_override,
            },
        }
    }

    pub fn get_node_type(&self) -> &NodeType {
        &self.deployment_aux_data.node_type
    }

    pub fn application_config_values(&self) -> IndexMap<NodeService, Value> {
        let component_configs = self.deployment_aux_data.node_type.get_component_configs(None);
        let mut result = IndexMap::new();

        for (service, component_config) in component_configs.into_iter() {
            // Component configs, determined by the service.
            let component_config_serialization_wrapper: ComponentConfigsSerializationWrapper =
                component_config.into();

            let flattened_component_config_map =
                config_to_preset(&json!(component_config_serialization_wrapper.dump()));
            result.insert(service, flattened_component_config_map);
        }

        result
    }

    pub fn get_all_services_config_paths(&self) -> Vec<Vec<String>> {
        self.services.iter().map(|service| service.get_service_config_paths()).collect()
    }

    pub fn deployment_file_path(&self) -> PathBuf {
        self.deployment_aux_data
            .environment
            .env_dir_path()
            .join(format!("deployment_config_{}.json", self.deployment_aux_data.instance_name))
    }

    pub fn dump_config_override_files(&self) {
        self.deployment_aux_data.config_override.dump_config_files(
            &self.deployment_aux_data.environment.env_dir_path(),
            &self.deployment_aux_data.instance_name,
        );
    }

    #[cfg(test)]
    pub fn test_dump_config_override_files(&self) {
        self.deployment_aux_data.config_override.test_dump_config_files(
            &self.deployment_aux_data.environment.env_dir_path(),
            &self.deployment_aux_data.instance_name,
        );
    }
}

#[derive(Clone, Debug)]
struct DeploymentAuxData {
    node_type: NodeType,
    environment: Environment,
    instance_name: String,
    config_override: ConfigOverride,
}

// Creates the service name in the format: <node_service>.<namespace>.<domain>
pub(crate) fn build_service_namespace_domain_address(
    node_service: &str,
    namespace: &str,
    domain: &str,
) -> String {
    format!("{}.{}.{}", node_service, namespace, domain)
}

// TODO(Tsabary): when transitioning runnings nodes in different clusters, this enum should be
// removed, and the p2p address should always be `External`.
#[derive(Clone)]
pub enum P2PCommunicationType {
    Internal,
    External,
}

impl P2PCommunicationType {
    pub(crate) fn get_p2p_domain(&self, domain: &str) -> String {
        match self {
            P2PCommunicationType::Internal => "svc.cluster.local",
            P2PCommunicationType::External => domain,
        }
        .to_string()
    }

    pub(crate) fn get_k8s_service_type(&self) -> K8SServiceType {
        K8SServiceType::LoadBalancer
    }
}

// TODO(Tsabary): move this to the service module once refactored out of here.
// A helper struct for serializing the components config in the same hierarchy as of its
// serialization as part of the entire config, i.e., by prepending "components.".
#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct ComponentConfigsSerializationWrapper {
    components: ComponentConfig,
}

impl From<ComponentConfig> for ComponentConfigsSerializationWrapper {
    fn from(value: ComponentConfig) -> Self {
        ComponentConfigsSerializationWrapper { components: value }
    }
}

impl SerializeConfig for ComponentConfigsSerializationWrapper {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        prepend_sub_config_name(self.components.dump(), "components")
    }
}
