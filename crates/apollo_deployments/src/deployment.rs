use std::path::PathBuf;

use apollo_infra_utils::dumping::serialize_to_file;
#[cfg(test)]
use apollo_infra_utils::dumping::serialize_to_file_test;
use apollo_monitoring_endpoint::config::MonitoringEndpointConfig;
use apollo_node::config::config_utils::{get_deployment_from_config_path, BaseAppConfigOverride};
use indexmap::IndexMap;
use serde::Serialize;
use serde_json::Value;
use starknet_api::core::ChainId;

use crate::deployment_definitions::{Environment, CONFIG_BASE_DIR};
use crate::service::{DeploymentName, ExternalSecret, Service, ServiceName};

const DEPLOYMENT_IMAGE: &str =
    "ghcr.io/starkware-libs/sequencer/sequencer:\
     04-10-chore_apollo_deployments_3_nodes_integration_deployments-1a9c48e";
const DEPLOYMENT_CONFIG_DIR_NAME: &str = "deployment_configs/";

pub struct DeploymentAndPreset {
    deployment: Deployment,
    // TODO(Tsabary): consider using PathBuf instead.
    dump_file_path: PathBuf,
    base_app_config_file_path: &'static str,
}

impl DeploymentAndPreset {
    pub fn new(deployment: Deployment, base_app_config_file_path: &'static str) -> Self {
        let dump_file_path = deployment.deployment_file_path();
        Self { deployment, dump_file_path, base_app_config_file_path }
    }

    pub fn get_deployment(&self) -> &Deployment {
        &self.deployment
    }

    pub fn get_dump_file_path(&self) -> PathBuf {
        self.dump_file_path.clone()
    }

    pub fn get_base_app_config_file_path(&self) -> &'static str {
        self.base_app_config_file_path
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Deployment {
    chain_id: ChainId,
    image: &'static str,
    application_config_subdir: PathBuf,
    #[serde(skip_serializing)]
    deployment_name: DeploymentName,
    #[serde(skip_serializing)]
    environment: Environment,
    services: Vec<Service>,
    #[serde(skip_serializing)]
    instance_name: String,
}

impl Deployment {
    pub fn new(
        chain_id: ChainId,
        deployment_name: DeploymentName,
        environment: Environment,
        instance_name: &str,
        external_secret: Option<ExternalSecret>,
    ) -> Self {
        let service_names = deployment_name.all_service_names();
        let services = service_names
            .iter()
            .map(|service_name| service_name.create_service(&environment, &external_secret))
            .collect();
        Self {
            chain_id,
            image: DEPLOYMENT_IMAGE,
            application_config_subdir: deployment_name
                .add_path_suffix(environment.application_config_dir_path(), instance_name),
            deployment_name,
            environment,
            services,
            instance_name: instance_name.to_string(),
        }
    }

    pub fn get_deployment_name(&self) -> &DeploymentName {
        &self.deployment_name
    }

    pub fn application_config_values(
        &self,
        base_app_config_file_path: &str,
    ) -> IndexMap<ServiceName, Value> {
        let deployment_base_app_config = get_deployment_from_config_path(base_app_config_file_path);
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

    pub fn dump_application_config_files(&self, base_app_config_file_path: &str) {
        let app_configs = self.application_config_values(base_app_config_file_path);
        for (service, value) in app_configs.into_iter() {
            let config_path = &self.application_config_subdir.join(service.get_config_file_path());
            serialize_to_file(
                value,
                config_path.to_str().expect("Should be able to convert path to string"),
            );
        }
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
            // Concatenate paths.
            let full_path = &self.application_config_subdir.join(service.get_config_path());
            // Assert existence.
            assert!(full_path.exists(), "File does not exist: {:?}", full_path);
        }
    }

    #[cfg(test)]
    pub fn test_dump_application_config_files(&self, base_app_config_file_path: &str) {
        let app_configs = self.application_config_values(base_app_config_file_path);
        for (service, value) in app_configs.into_iter() {
            let config_path = &self.application_config_subdir.join(service.get_config_file_path());
            serialize_to_file_test(
                value,
                config_path.to_str().expect("Should be able to convert path to string"),
            );
        }
    }
}
