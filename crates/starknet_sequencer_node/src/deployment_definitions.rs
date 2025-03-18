use starknet_api::core::ChainId;

use crate::deployment::{Deployment, DeploymentAndPreset, DeploymentName};

#[cfg(test)]
#[path = "deployment_definitions_test.rs"]
mod deployment_definitions_test;

const DEPLOYMENT_IMAGE: &str = "ghcr.io/starkware-libs/sequencer/sequencer:dev";

// TODO(Tsabary): decide on the dir structure and naming convention for the deployment presets.

// TODO(Tsabary): temporarily moved this definition here, to include it in the deployment.
pub const SINGLE_NODE_CONFIG_PATH: &str =
    "config/sequencer/presets/system_test_presets/single_node/node_0/executable_0/node_config.json";

// TODO(Tsabary): fill and order these.

type DeploymentFn = fn() -> DeploymentAndPreset;

pub const DEPLOYMENTS: &[DeploymentFn] = &[create_main_deployment, create_testing_deployment];

fn create_main_deployment() -> DeploymentAndPreset {
    DeploymentAndPreset::new(
        Deployment::new(
            ChainId::Mainnet,
            DEPLOYMENT_IMAGE,
            "config/sequencer/presets/system_test_presets/single_node/",
            DeploymentName::DistributedNode,
        ),
        "config/sequencer/presets/main.json",
    )
}

fn create_testing_deployment() -> DeploymentAndPreset {
    DeploymentAndPreset::new(
        Deployment::new(
            ChainId::IntegrationSepolia,
            DEPLOYMENT_IMAGE,
            "config/sequencer/presets/system_test_presets/single_node/",
            DeploymentName::ConsolidatedNode,
        ),
        "config/sequencer/deployment_configs/testing/nightly_test_all_in_one.json",
    )
}
