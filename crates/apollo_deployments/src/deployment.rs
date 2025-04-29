use std::path::{Path, PathBuf};

use apollo_infra_utils::dumping::serialize_to_file;
#[cfg(test)]
use apollo_infra_utils::dumping::serialize_to_file_test;
use apollo_monitoring_endpoint::config::MonitoringEndpointConfig;
use apollo_node::config::config_utils::{get_deployment_from_config_path, BaseAppConfigOverride};
use indexmap::IndexMap;
use serde::Serialize;
use serde_json::{to_value, Value};
use starknet_api::core::ChainId;

use crate::deployment_definitions::{Environment, CONFIG_BASE_DIR};
use crate::service::{DeploymentName, ExternalSecret, Service, ServiceName};

#[cfg(test)]
pub(crate) const FIX_BINARY_NAME: &str = "deployment_generator";

const DEPLOYMENT_CONFIG_DIR_NAME: &str = "deployment_configs/";

const DEPLOYMENT_FILE_NAME: &str = "deployment_config_override.json";
const INSTANCE_FILE_NAME: &str = "instance_config_override.json";

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Deployment {
    chain_id: ChainId,
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

// TODO(Tsabary): reduce number of args.
#[allow(clippy::too_many_arguments)]
impl Deployment {
    pub fn new(
        chain_id: ChainId,
        deployment_name: DeploymentName,
        environment: Environment,
        instance_name: &str,
        external_secret: Option<ExternalSecret>,
        base_app_config_file_path: PathBuf,
        config_override: ConfigOverride,
        domain: String,
        ingress_alternative_names: Option<Vec<String>>,
    ) -> Self {
        let service_names = deployment_name.all_service_names();

        let application_config_subdir = deployment_name
            .add_path_suffix(environment.application_config_dir_path(), instance_name);

        let additional_config_filenames: Vec<String> =
            config_override.create(&application_config_subdir);

        let services = service_names
            .iter()
            .map(|service_name| {
                service_name.create_service(
                    &environment,
                    &external_secret,
                    additional_config_filenames.clone(),
                    domain.clone(),
                    ingress_alternative_names.clone(),
                )
            })
            .collect();
        Self {
            chain_id,
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
        let deployment_base_app_config =
            get_deployment_from_config_path(self.get_base_app_config_file_path().to_str().unwrap());
        let component_configs = self.deployment_name.get_component_configs(None, &self.environment);

        let mut result = IndexMap::new();

        for (service, component_config) in component_configs.into_iter() {
            let mut service_deployment_base_app_config = deployment_base_app_config.clone();

            let monitoring_endpoint_config = MonitoringEndpointConfig::deployment();
            let base_app_config_override =
                BaseAppConfigOverride::new(component_config, monitoring_endpoint_config);

            service_deployment_base_app_config.override_base_app_config(base_app_config_override);

            result.insert(service, service_deployment_base_app_config.as_value());
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
// TODO(Tsabary): modify the loading test of the base app config to include the overrides as well.
// TODO(Tsabary): delete duplicates from the base app config, and add a test that there are no
// conflicts between all the override config entries and the values in the base app config.

#[derive(Debug, Serialize)]
pub struct ConfigOverride {
    deployment_config_override: &'static DeploymentConfigOverride,
    instance_config_override: &'static InstanceConfigOverride,
}

impl ConfigOverride {
    pub const fn new(
        deployment_config_override: &'static DeploymentConfigOverride,
        instance_config_override: &'static InstanceConfigOverride,
    ) -> Self {
        Self { deployment_config_override, instance_config_override }
    }

    pub fn create(&self, application_config_subdir: &Path) -> Vec<String> {
        serialize_to_file(
            to_value(self.deployment_config_override).unwrap(),
            application_config_subdir.join(DEPLOYMENT_FILE_NAME).to_str().unwrap(),
        );

        serialize_to_file(
            to_value(self.instance_config_override).unwrap(),
            application_config_subdir.join(INSTANCE_FILE_NAME).to_str().unwrap(),
        );
        vec![DEPLOYMENT_FILE_NAME.to_string(), INSTANCE_FILE_NAME.to_string()]
    }
}

#[derive(Debug, Serialize)]
pub struct DeploymentConfigOverride {
    #[serde(rename = "base_layer_config.starknet_contract_address")]
    starknet_contract_address: &'static str,
    chain_id: &'static str,
    eth_fee_token_address: &'static str,
    starknet_url: &'static str,
    strk_fee_token_address: &'static str,
}

impl DeploymentConfigOverride {
    pub const fn new(
        starknet_contract_address: &'static str,
        chain_id: &'static str,
        eth_fee_token_address: &'static str,
        starknet_url: &'static str,
        strk_fee_token_address: &'static str,
    ) -> Self {
        Self {
            starknet_contract_address,
            chain_id,
            eth_fee_token_address,
            starknet_url,
            strk_fee_token_address,
        }
    }
}

// TODO(Tsabary): re-verify all config diffs.

#[derive(Debug, Serialize)]
pub struct InstanceConfigOverride {
    #[serde(rename = "consensus_manager_config.network_config.bootstrap_peer_multiaddr")]
    consensus_bootstrap_peer_multiaddr: &'static str,
    #[serde(rename = "consensus_manager_config.network_config.bootstrap_peer_multiaddr.#is_none")]
    consensus_bootstrap_peer_multiaddr_is_none: bool,
    // TODO(Tsabary): network secret keys should be defined as secrets.
    #[serde(rename = "consensus_manager_config.network_config.secret_key")]
    consensus_secret_key: &'static str,
    #[serde(rename = "mempool_p2p_config.network_config.bootstrap_peer_multiaddr")]
    mempool_bootstrap_peer_multiaddr: &'static str,
    #[serde(rename = "mempool_p2p_config.network_config.bootstrap_peer_multiaddr.#is_none")]
    mempool_bootstrap_peer_multiaddr_is_none: bool,
    // TODO(Tsabary): network secret keys should be defined as secrets.
    #[serde(rename = "mempool_p2p_config.network_config.secret_key")]
    mempool_secret_key: &'static str,
    validator_id: &'static str,
}

impl InstanceConfigOverride {
    pub const fn new(
        consensus_bootstrap_peer_multiaddr: &'static str,
        consensus_bootstrap_peer_multiaddr_is_none: bool,
        consensus_secret_key: &'static str,
        mempool_bootstrap_peer_multiaddr: &'static str,
        mempool_bootstrap_peer_multiaddr_is_none: bool,
        mempool_secret_key: &'static str,
        validator_id: &'static str,
    ) -> Self {
        Self {
            consensus_bootstrap_peer_multiaddr,
            consensus_bootstrap_peer_multiaddr_is_none,
            consensus_secret_key,
            mempool_bootstrap_peer_multiaddr,
            mempool_bootstrap_peer_multiaddr_is_none,
            mempool_secret_key,
            validator_id,
        }
    }
}
