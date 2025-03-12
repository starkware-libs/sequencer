use starknet_api::core::ChainId;

use crate::deployment::{Deployment, Service, ServiceName};

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
pub const MAIN_DEPLOYMENT: Deployment<'_> = Deployment::new(
    ChainId::Mainnet,
    DEPLOYMENT_IMAGE,
    &[BATCHER_MAIN, GATEWAY_MAIN, MEMPOOL_MAIN],
);

pub const TESTING_DEPLOYMENT_PRESET_PATH: &str =
    "config/sequencer/deployment_configs/testing/nightly_test_all_in_one.json";
pub const TESTING_DEPLOYMENT: Deployment<'_> =
    Deployment::new(ChainId::IntegrationSepolia, DEPLOYMENT_IMAGE, &[ALL_IN_ONE_TESTING]);

// Main deployment services.
const BATCHER_MAIN: Service = Service::new(ServiceName::Batcher, "", false, false, 1, Some(500));
const GATEWAY_MAIN: Service = Service::new(ServiceName::Gateway, "", false, true, 1, None);
const MEMPOOL_MAIN: Service = Service::new(ServiceName::Mempool, "", false, false, 1, None);

// Test deployment services.
const ALL_IN_ONE_TESTING: Service =
    Service::new(ServiceName::AllInOne, SINGLE_NODE_CONFIG_PATH, false, false, 1, Some(32));
