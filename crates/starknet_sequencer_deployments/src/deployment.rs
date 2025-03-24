#[cfg(test)]
use std::path::Path;

use serde::Serialize;
use starknet_api::core::ChainId;

use crate::service::{DeploymentName, IntoService, Service};

const DEPLOYMENT_IMAGE: &str = "ghcr.io/starkware-libs/sequencer/sequencer:dev";

pub struct DeploymentAndPreset {
    pub deployment: Deployment,
    pub dump_file_path: &'static str,
}

impl DeploymentAndPreset {
    pub fn new(deployment: Deployment, dump_file_path: &'static str) -> Self {
        Self { deployment, dump_file_path }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Deployment {
    chain_id: ChainId,
    image: &'static str,
    application_config_subdir: String,
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
            services,
        }
    }

    #[cfg(test)]
    pub fn assert_application_configs_exist(&self) {
        // TODO(Tsabary): avoid cloning here.
        for service in self.services.clone() {
            // Concatenate paths.
            let subdir_path = Path::new(&self.application_config_subdir);
            let full_path = subdir_path.join(service.get_config_path());
            // Assert existence.
            assert!(full_path.exists(), "File does not exist: {:?}", full_path);
        }
    }
}
