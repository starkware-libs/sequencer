use starknet_api::core::ChainId;

use crate::deployment::{Deployment, DeploymentName};

#[cfg(test)]
#[path = "deployment_definitions_test.rs"]
mod deployment_definitions_test;

const DEPLOYMENT_IMAGE: &str = "ghcr.io/starkware-libs/sequencer/sequencer:dev";

// TODO(Tsabary): decide on the dir structure and naming convention for the deployment presets.

// TODO(Tsabary): temporarily moved this definition here, to include it in the deployment.
pub const SINGLE_NODE_CONFIG_PATH: &str =
    "config/sequencer/presets/system_test_presets/single_node/node_0/executable_0/node_config.json";

// TODO(Tsabary): fill and order these.
pub const MAIN_DEPLOYMENT_PRESET_PATH: &str = "config/sequencer/presets/main.json";
pub const MAIN_DEPLOYMENT_APP_CONFIG_SUBDIR: &str =
    "config/sequencer/presets/system_test_presets/single_node/";

pub fn create_main_deployment() -> Deployment<'static> {
    Deployment::new(
        ChainId::Mainnet,
        DEPLOYMENT_IMAGE,
        MAIN_DEPLOYMENT_APP_CONFIG_SUBDIR,
        DeploymentName::DistributedNode,
    )
}

pub const TESTING_DEPLOYMENT_PRESET_PATH: &str =
    "config/sequencer/deployment_configs/testing/nightly_test_all_in_one.json";
pub const TESTING_DEPLOYMENT_APP_CONFIG_SUBDIR: &str =
    "config/sequencer/presets/system_test_presets/single_node/";

pub fn create_testing_deployment() -> Deployment<'static> {
    Deployment::new(
        ChainId::IntegrationSepolia,
        DEPLOYMENT_IMAGE,
        TESTING_DEPLOYMENT_APP_CONFIG_SUBDIR,
        DeploymentName::ConsolidatedNode,
    )
}
