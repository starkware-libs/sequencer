use std::path::PathBuf;

use apollo_infra::component_definitions::{LocalServerConfig, RemoteClientConfig};
use starknet_api::core::ChainId;
use strum_macros::{Display, EnumString};

use crate::deployment::{Deployment, DeploymentAndPreset};
use crate::service::DeploymentName;

#[cfg(test)]
#[path = "deployment_definitions_test.rs"]
mod deployment_definitions_test;

// TODO(Tsabary): separate deployments to different modules.

const SYSTEM_TEST_BASE_APP_CONFIG_PATH: &str =
    "config/sequencer/testing/base_app_configs/single_node_deployment_test.json";

const INTEGRATION_BASE_APP_CONFIG_PATH: &str =
    "config/sequencer/sepolia_integration/base_app_configs/config.json";

const CONFIG_BASE_DIR: &str = "config/sequencer/";
const DEPLOYMENT_CONFIG_DIR_NAME: &str = "deployment_configs/";
const APP_CONFIGS_DIR_NAME: &str = "app_configs/";

type DeploymentFn = fn() -> DeploymentAndPreset;

pub const DEPLOYMENTS: &[DeploymentFn] = &[
    system_test_distributed_deployment,
    system_test_consolidated_deployment,
    integration_consolidated_deployment,
];

// Integration deployments
fn integration_consolidated_deployment() -> DeploymentAndPreset {
    DeploymentAndPreset::new(
        Deployment::new(
            ChainId::IntegrationSepolia,
            DeploymentName::ConsolidatedNode,
            Environment::SepoliaIntegration,
        ),
        deployment_file_path(Environment::SepoliaIntegration, "integration_consolidated"),
        INTEGRATION_BASE_APP_CONFIG_PATH,
    )
}

// System test deployments
fn system_test_distributed_deployment() -> DeploymentAndPreset {
    DeploymentAndPreset::new(
        Deployment::new(
            ChainId::IntegrationSepolia,
            DeploymentName::DistributedNode,
            Environment::Testing,
        ),
        deployment_file_path(Environment::Testing, "deployment_test_distributed"),
        SYSTEM_TEST_BASE_APP_CONFIG_PATH,
    )
}

fn system_test_consolidated_deployment() -> DeploymentAndPreset {
    DeploymentAndPreset::new(
        Deployment::new(
            ChainId::IntegrationSepolia,
            DeploymentName::ConsolidatedNode,
            Environment::Testing,
        ),
        deployment_file_path(Environment::Testing, "deployment_test_consolidated"),
        SYSTEM_TEST_BASE_APP_CONFIG_PATH,
    )
}

#[derive(EnumString, Clone, Display, PartialEq, Debug)]
#[strum(serialize_all = "snake_case")]
pub enum Environment {
    Testing,
    SepoliaIntegration,
    SepoliaTestnet,
    Mainnet,
}

impl Environment {
    pub fn application_config_dir_path(&self) -> PathBuf {
        PathBuf::from(CONFIG_BASE_DIR).join(self.to_string()).join(APP_CONFIGS_DIR_NAME)
    }

    pub fn get_component_config_modifications(&self) -> EnvironmentComponentConfigModifications {
        match self {
            Environment::Testing => EnvironmentComponentConfigModifications::testing(),
            Environment::SepoliaIntegration => {
                EnvironmentComponentConfigModifications::sepolia_integration()
            }
            Environment::SepoliaTestnet => unimplemented!("SepoliaTestnet is not implemented yet"),
            Environment::Mainnet => unimplemented!("Mainnet is not implemented yet"),
        }
    }
}

pub struct EnvironmentComponentConfigModifications {
    pub local_server_config: LocalServerConfig,
    pub max_concurrency: usize,
    pub remote_client_config: RemoteClientConfig,
}

impl EnvironmentComponentConfigModifications {
    pub fn testing() -> Self {
        Self {
            local_server_config: LocalServerConfig { channel_buffer_size: 32 },
            max_concurrency: 10,
            remote_client_config: RemoteClientConfig {
                retries: 3,
                idle_connections: 5,
                idle_timeout: 90,
                retry_interval: 3,
            },
        }
    }

    pub fn sepolia_integration() -> Self {
        Self {
            local_server_config: LocalServerConfig { channel_buffer_size: 128 },
            max_concurrency: 100,
            remote_client_config: RemoteClientConfig {
                retries: 3,
                idle_connections: usize::MAX,
                idle_timeout: 1,
                retry_interval: 1,
            },
        }
    }
}

pub fn deployment_file_path(environment: Environment, deployment_name: &str) -> PathBuf {
    PathBuf::from(CONFIG_BASE_DIR)
        .join(environment.to_string())
        .join(DEPLOYMENT_CONFIG_DIR_NAME)
        .join(format!("{deployment_name}.json"))
}
