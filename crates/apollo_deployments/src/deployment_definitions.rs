use std::path::PathBuf;

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

type DeploymentFn = fn() -> DeploymentAndPreset;

pub const DEPLOYMENTS: &[DeploymentFn] = &[
    system_test_distributed_deployment,
    system_test_consolidated_deployment,
    integration_consolidated_deployment,
];

// Integration deployments
fn integration_consolidated_deployment() -> DeploymentAndPreset {
    DeploymentAndPreset::new(
        Deployment::new(ChainId::IntegrationSepolia, DeploymentName::ConsolidatedNode),
        deployment_file_path(Environment::SepoliaIntegration, "integration_consolidated"),
        INTEGRATION_BASE_APP_CONFIG_PATH,
    )
}

// System test deployments
fn system_test_distributed_deployment() -> DeploymentAndPreset {
    DeploymentAndPreset::new(
        Deployment::new(ChainId::IntegrationSepolia, DeploymentName::DistributedNode),
        deployment_file_path(Environment::Testing, "deployment_test_distributed"),
        SYSTEM_TEST_BASE_APP_CONFIG_PATH,
    )
}

fn system_test_consolidated_deployment() -> DeploymentAndPreset {
    DeploymentAndPreset::new(
        Deployment::new(ChainId::IntegrationSepolia, DeploymentName::ConsolidatedNode),
        deployment_file_path(Environment::Testing, "deployment_test_consolidated"),
        SYSTEM_TEST_BASE_APP_CONFIG_PATH,
    )
}

#[derive(EnumString, Display, Debug)]
#[strum(serialize_all = "snake_case")]
pub(crate) enum Environment {
    Testing,
    SepoliaIntegration,
    SepoliaTestnet,
    Mainnet,
}

pub(crate) fn deployment_file_path(environment: Environment, deployment_name: &str) -> PathBuf {
    PathBuf::from(CONFIG_BASE_DIR)
        .join(environment.to_string())
        .join(DEPLOYMENT_CONFIG_DIR_NAME)
        .join(format!("{deployment_name}.json"))
}
