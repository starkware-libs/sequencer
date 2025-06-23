use std::collections::BTreeMap;
use std::fmt::{Display, Formatter, Result};
use std::path::{Path, PathBuf};

use apollo_config::dumping::{prepend_sub_config_name, SerializeConfig};
use apollo_config::{ParamPath, SerializedParam};
use apollo_infra_utils::dumping::serialize_to_file;
#[cfg(test)]
use apollo_infra_utils::dumping::serialize_to_file_test;
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

#[derive(Clone, Debug, PartialEq, Serialize)]
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

        let application_config_subdir = deployment_name
            .add_path_suffix(environment.application_config_dir_path(), instance_name);

        // TODO(Tsabary): list the mutual parent dir of the base app config and all the services'
        // configs as the parent dir, and for each file add its specific path originating from that
        // dir. This will enable removing the current "upward" paths.

        // Reference the base app config file from the application config subdir.
        let base_app_config_relative_path =
            relative_up_path(&application_config_subdir, &base_app_config_file_path);

        let config_override_files: Vec<String> = config_override.create(&application_config_subdir);

        let additional_config_filenames: Vec<String> =
            std::iter::once(base_app_config_relative_path.to_string_lossy().to_string())
                .chain(config_override_files)
                .collect();

        let services = service_names
            .iter()
            .map(|service_name| {
                service_name.create_service(
                    &environment,
                    &external_secret,
                    additional_config_filenames.clone(),
                    ingress_params.clone(),
                    k8s_service_config_params.clone(),
                )
            })
            .collect();
        Self {
            application_config_subdir,
            deployment_name,
            environment,
            services,
            instance_name: instance_name.to_string(),
            base_app_config_file_path,
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

    pub fn dump_application_config_files(&self) {
        let app_configs = self.application_config_values();
        for (service, value) in app_configs.into_iter() {
            let config_path = &self.application_config_subdir.join(service.get_config_file_path());
            serialize_to_file(
                value,
                config_path.to_str().expect("Should be able to convert path to string"),
            );
        }
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

    #[cfg(test)]
    pub(crate) fn assert_application_configs_exist(&self) {
        for service in &self.services {
            for config_path in service.get_config_paths() {
                // Concatenate paths.
                let full_path = &self.application_config_subdir.join(config_path);
                // Assert existence.
                assert!(full_path.exists(), "File does not exist: {:?}", full_path);
            }
        }
    }

    #[cfg(test)]
    pub fn test_dump_application_config_files(&self) {
        let app_configs = self.application_config_values();
        for (service, value) in app_configs.into_iter() {
            let config_path = &self.application_config_subdir.join(service.get_config_file_path());
            serialize_to_file_test(
                value,
                config_path.to_str().expect("Should be able to convert path to string"),
                FIX_BINARY_NAME,
            );
        }
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

fn relative_up_path(from: &Path, to: &Path) -> PathBuf {
    // Canonicalize logically (NOT on filesystem)
    let from_components: Vec<_> = from.components().collect();
    let to_components: Vec<_> = to.components().collect();

    // Find common prefix length
    let common_len = from_components.iter().zip(&to_components).take_while(|(a, b)| a == b).count();

    // How many directories to go up from `from` to get to common root
    let up_levels = from_components.len() - common_len;

    // Build the relative path
    let mut result = PathBuf::new();
    for _ in 0..up_levels {
        result.push("..");
    }
    for component in &to_components[common_len..] {
        result.push(component.as_os_str());
    }

    result
}

// A helper struct for serializing the components config in the same hierarchy as of its
// serialization as part of the entire config, i.e., by prepending "components.".
#[derive(Clone, Debug, Default, Serialize)]
struct ComponentConfigsSerializationWrapper {
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
