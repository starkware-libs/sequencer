use starknet_api::core::ChainId;

use crate::deployment::{Deployment, DeploymentAndPreset};
use crate::service::DeploymentName;

#[cfg(test)]
#[path = "deployment_definitions_test.rs"]
mod deployment_definitions_test;

// TODO(Tsabary): temporarily moved this definition here, to include it in the deployment.
pub const SINGLE_NODE_CONFIG_PATH: &str =
    "config/sequencer/presets/system_test_presets/single_node/node_0/executable_0/node_config.json";

type DeploymentFn = fn() -> DeploymentAndPreset;

pub const DEPLOYMENTS: &[DeploymentFn] = &[create_main_deployment, create_testing_deployment];

// TODO(Tsabary): for all deployments, update name, all config file paths, and service values.
fn create_main_deployment() -> DeploymentAndPreset {
    DeploymentAndPreset::new(
        Deployment::new(
            ChainId::IntegrationSepolia,
            DeploymentName::DistributedNode,
            SINGLE_NODE_CONFIG_PATH.into(),
        ),
        "config/sequencer/deployment_configs/testing/nightly_test_distributed_node.json",
    )
}

fn create_testing_deployment() -> DeploymentAndPreset {
    DeploymentAndPreset::new(
        Deployment::new(
            ChainId::IntegrationSepolia,
            DeploymentName::ConsolidatedNode,
            SINGLE_NODE_CONFIG_PATH.into(),
        ),
        "config/sequencer/deployment_configs/testing/nightly_test_all_in_one.json",
    )
}
