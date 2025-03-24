#[cfg(test)]
use std::path::Path;

use serde::Serialize;
use starknet_api::core::ChainId;

use crate::service::{DeploymentName, IntoService, Service};

const DEPLOYMENT_IMAGE: &str = "ghcr.io/starkware-libs/sequencer/sequencer:dev";

pub struct DeploymentAndPreset {
    deployment: Deployment,
    // TODO(Tsabary): consider using PathBuf instead.
    dump_file_path: &'static str,
    base_app_config_file_path: &'static str,
}

impl DeploymentAndPreset {
    pub fn new(
        deployment: Deployment,
        dump_file_path: &'static str,
        base_app_config_file_path: &'static str,
    ) -> Self {
        Self { deployment, dump_file_path, base_app_config_file_path }
    }

    pub fn get_deployment(&self) -> &Deployment {
        &self.deployment
    }

    pub fn get_dump_file_path(&self) -> &'static str {
        self.dump_file_path
    }

    pub fn get_base_app_config_file_path(&self) -> &'static str {
        self.base_app_config_file_path
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Deployment {
    chain_id: ChainId,
    image: &'static str,
    application_config_subdir: String,
    #[serde(skip_serializing)]
    deployment_name: DeploymentName,
    services: Vec<Service>,
}

impl Deployment {
    pub fn new(chain_id: ChainId, deployment_name: DeploymentName) -> Self {
        let service_names = deployment_name.all_service_names();
        let services =
            service_names.iter().map(|service_name| service_name.create_service()).collect();
        Self {
            chain_id,
            image: DEPLOYMENT_IMAGE,
            application_config_subdir: deployment_name.get_path(),
            deployment_name,
            services,
        }
    }

    pub fn dump_application_config_files(&self, _base_app_config_file_path: &str) {
        // TODO(Tsabary): load the base app config, and create a DeploymentBaseAppConfig instance.
        let component_configs = self.deployment_name.get_component_configs(None);

        // Iterate over the component_configs
        for (_service, _component_config) in component_configs.iter() {
            // TODO(Tsabary): clone the loaded DeploymentBaseAppConfig instance.
            // TODO(Tsabary): replace the current component config into the cloned
            // DeploymentBaseAppConfig instance. TODO(Tsabary): dump the modified
            // instance to the service path.
        }
    }

    #[cfg(test)]
    pub(crate) fn assert_application_configs_exist(&self) {
        for service in &self.services {
            // Concatenate paths.
            let subdir_path = Path::new(&self.application_config_subdir);
            let full_path = subdir_path.join(service.get_config_path());
            // Assert existence.
            assert!(full_path.exists(), "File does not exist: {:?}", full_path);
        }
    }
}
