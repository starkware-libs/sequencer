use std::path::PathBuf;

use starknet_api::core::ChainId;
use strum_macros::{Display, EnumString};

use crate::deployment::{Deployment, DeploymentAndPreset};
use crate::service::DeploymentName;

#[cfg(test)]
#[path = "deployment_definitions_test.rs"]
mod deployment_definitions_test;

// TODO(Tsabary): temporarily moved this definition here, to include it in the deployment.
pub const SINGLE_NODE_CONFIG_PATH: &str =
    "config/sequencer/presets/system_test_presets/single_node/node_0/executable_0/node_config.json";

type DeploymentFn = fn() -> DeploymentAndPreset;

pub const DEPLOYMENTS: &[DeploymentFn] =
    &[system_test_distributed_deployment, system_test_consolidated_deployment];

fn system_test_distributed_deployment() -> DeploymentAndPreset {
    DeploymentAndPreset::new(
        Deployment::new(ChainId::IntegrationSepolia, DeploymentName::DistributedNode),
        deployment_file_path(Environment::Testing, "deployment_test_distributed"),
        SINGLE_NODE_CONFIG_PATH,
    )
}

fn system_test_consolidated_deployment() -> DeploymentAndPreset {
    DeploymentAndPreset::new(
        Deployment::new(ChainId::IntegrationSepolia, DeploymentName::ConsolidatedNode),
        deployment_file_path(Environment::Testing, "deployment_test_consolidated"),
        SINGLE_NODE_CONFIG_PATH,
    )
}

const CONFIG_BASE_DIR: &str = "config/sequencer/";

const DEPLOYMENT_CONFIG_DIR_NAME: &str = "deployment_configs/";

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
