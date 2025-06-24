use std::collections::BTreeMap;
use std::fmt::{Display, Formatter, Result};
use std::iter::once;
use std::path::PathBuf;

use apollo_config::dumping::{prepend_sub_config_name, SerializeConfig};
use apollo_config::{ParamPath, SerializedParam};
use apollo_node::config::component_config::ComponentConfig;
use apollo_node::config::config_utils::config_to_preset;
use apollo_protobuf::consensus::DEFAULT_VALIDATOR_ID;
use indexmap::IndexMap;
use serde::Serialize;
use serde_json::{json, Value};

use crate::config_override::ConfigOverride;
use crate::deployment_definitions::{Environment, CONFIG_BASE_DIR};
use crate::k8s::{ExternalSecret, IngressParams, K8SServiceType, K8sServiceConfigParams};
use crate::service::{DeploymentName, Service, ServiceName};

#[cfg(test)]
pub(crate) const FIX_BINARY_NAME: &str = "deployment_generator";

const DEPLOYMENT_CONFIG_DIR_NAME: &str = "deployment_configs/";

// TODO(Tsabary): almost all struct members are not serialized, causing many skip_serializing
// attributes. Consider splitting to inner structs.
// TODO(Tsabary): revisit derived traits, recursively remove from inner types if possible.
#[derive(Clone, Debug, Serialize)]
pub struct Deployment {
    application_config_subdir: PathBuf,
    #[serde(skip_serializing)]
    deployment_name: DeploymentName,
    #[serde(skip_serializing)]
    environment: Environment,
    services: Vec<Service>,
    #[serde(skip_serializing)]
    instance_name: String,
    #[serde(skip_serializing)]
    base_app_config_file_path: PathBuf,
    #[serde(skip_serializing)]
    config_override: ConfigOverride,
    #[serde(skip_serializing)]
    config_override_dir: PathBuf,
}

impl Deployment {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        deployment_name: DeploymentName,
        environment: Environment,
        instance_name: &str,
        external_secret: Option<ExternalSecret>,
        base_app_config_file_path: PathBuf,
        config_override: ConfigOverride,
        ingress_params: IngressParams,
        k8s_service_config_params: Option<K8sServiceConfigParams>,
    ) -> Self {
        let service_names = deployment_name.all_service_names();

        let config_override_dir = deployment_name
            .add_path_suffix(environment.application_config_dir_path(), instance_name);

        let config_override_files = config_override.get_config_file_paths(&config_override_dir);
        let config_filenames: Vec<String> =
            once(base_app_config_file_path.to_string_lossy().to_string())
                .chain(config_override_files)
                .collect();

        let services = service_names
            .iter()
            .map(|service_name| {
                service_name.create_service(
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
            deployment_name,
            environment,
            services,
            instance_name: instance_name.to_string(),
            base_app_config_file_path,
            config_override,
            config_override_dir,
        }
    }

    pub fn get_deployment_name(&self) -> &DeploymentName {
        &self.deployment_name
    }

    pub fn get_base_app_config_file_path(&self) -> PathBuf {
        self.base_app_config_file_path.clone()
    }

    pub fn application_config_values(&self) -> IndexMap<ServiceName, Value> {
        let component_configs = self.deployment_name.get_component_configs(None);
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

    pub fn get_config_file_paths(&self) -> Vec<Vec<String>> {
        self.services
            .iter()
            .map(|service| {
                service
                    .get_config_paths()
                    .into_iter()
                    .map(|s| format!("{}{}", self.application_config_subdir.to_string_lossy(), s))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    pub fn deployment_file_path(&self) -> PathBuf {
        PathBuf::from(CONFIG_BASE_DIR)
            .join(self.environment.to_string())
            .join(DEPLOYMENT_CONFIG_DIR_NAME)
            .join(format!("{}.json", self.instance_name))
    }

    pub fn dump_config_override_files(&self) {
        self.config_override.dump_config_files(&self.config_override_dir);
    }

    #[cfg(test)]
    pub fn test_dump_config_override_files(&self) {
        self.config_override.test_dump_config_files(&self.config_override_dir);
    }
}

// TODO(Tsabary): test no conflicts between config entries defined in each of the override types.
// TODO(Tsabary): delete duplicates from the base app config, and add a test that there are no
// conflicts between all the override config entries and the values in the base app config.

/// Represents the domain of the pragma directive in the configuration.
pub enum PragmaDomain {
    Dev,
    Prod,
}

impl Display for PragmaDomain {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let s = match self {
            PragmaDomain::Dev => "devnet",
            PragmaDomain::Prod => "production",
        };
        write!(f, "{}", s)
    }
}

pub(crate) enum DeploymentType {
    Bootstrap,
    Operational,
}

impl DeploymentType {
    pub(crate) fn validator_id_offset(&self) -> usize {
        match self {
            DeploymentType::Bootstrap => 1,
            DeploymentType::Operational => DEFAULT_VALIDATOR_ID.try_into().unwrap(),
        }
    }
}

// Creates the service name in the format: <service_name>.<namespace>.<domain>
pub(crate) fn build_service_namespace_domain_address(
    service_name: &str,
    namespace: &str,
    domain: &str,
) -> String {
    format!("{}.{}.{}", service_name, namespace, domain)
}

// TODO(Tsabary): when transitioning runnings nodes in different clusters, this enum should be
// removed, and the p2p address should always be `External`.
#[derive(Clone)]
pub enum P2PCommunicationType {
    Internal,
    External,
}

impl P2PCommunicationType {
    pub(crate) fn get_p2p_address(
        &self,
        service_name: &str,
        namespace: &str,
        domain: &str,
        port: u16,
        first_node_address: &str,
    ) -> String {
        let domain = match self {
            P2PCommunicationType::Internal => "svc.cluster.local",
            P2PCommunicationType::External => domain,
        };

        let service_namespace_domain =
            build_service_namespace_domain_address(service_name, namespace, domain);
        format!("/dns/{}/tcp/{}/p2p/{}", service_namespace_domain, port, first_node_address)
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
