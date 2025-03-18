use starknet_api::core::ChainId;

use crate::deployment::{
    ConsolidatedNodeServiceName,
    Deployment,
    DistributedNodeServiceName,
    Service,
    ServiceName,
};

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
pub const MAIN_DEPLOYMENT: Deployment<'_> = Deployment::new(
    ChainId::Mainnet,
    DEPLOYMENT_IMAGE,
    MAIN_DEPLOYMENT_APP_CONFIG_SUBDIR,
    &[BATCHER_MAIN, GATEWAY_MAIN, MEMPOOL_MAIN],
);

pub const TESTING_DEPLOYMENT_PRESET_PATH: &str =
    "config/sequencer/deployment_configs/testing/nightly_test_all_in_one.json";
pub const TESTING_DEPLOYMENT_APP_CONFIG_SUBDIR: &str =
    "config/sequencer/presets/system_test_presets/single_node/";
pub const TESTING_DEPLOYMENT: Deployment<'_> = Deployment::new(
    ChainId::IntegrationSepolia,
    DEPLOYMENT_IMAGE,
    TESTING_DEPLOYMENT_APP_CONFIG_SUBDIR,
    &[CONSOLIDATED_TESTING],
);

// Main deployment services.
// TODO(Tsabary): fill in correct application configs.
const BATCHER_MAIN: Service = Service::new(
    ServiceName::DistributedNode(DistributedNodeServiceName::Batcher),
    "node_0/executable_0/node_config.json",
    false,
    false,
    1,
    Some(500),
);
const GATEWAY_MAIN: Service = Service::new(
    ServiceName::DistributedNode(DistributedNodeServiceName::Gateway),
    "node_0/executable_0/node_config.json",
    false,
    true,
    1,
    None,
);
const MEMPOOL_MAIN: Service = Service::new(
    ServiceName::DistributedNode(DistributedNodeServiceName::Mempool),
    "node_0/executable_0/node_config.json",
    false,
    false,
    1,
    None,
);

// Test deployment services.
// TODO(Tsabary): avoid the hard-coded path.
const CONSOLIDATED_TESTING: Service = Service::new(
    ServiceName::ConsolidatedNode(ConsolidatedNodeServiceName::Node),
    "node_0/executable_0/node_config.json",
    false,
    false,
    1,
    Some(32),
);
