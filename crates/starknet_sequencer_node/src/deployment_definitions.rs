use starknet_api::core::ChainId;

use crate::deployment::{Deployment, Service, ServiceName};

// TODO(Tsabary): decide on the dir structure and naming convention for the deployment presets.

// TODO(Tsabary): fill and order these.

#[cfg(test)]
#[path = "deployment_definitions_test.rs"]
mod deployment_definitions_test;

pub const MAIN_DEPLOYMENT_PRESET_PATH: &str = "config/sequencer/presets/main.json";

const BATCHER_MAIN: Service = Service::new(ServiceName::Batcher, "", false, false, 1, Some(500));

const GATEWAY_MAIN: Service = Service::new(ServiceName::Gateway, "", false, true, 1, None);

const MEMPOOL_MAIN: Service = Service::new(ServiceName::Mempool, "", false, false, 1, None);

pub const MAIN_DEPLOYMENT: Deployment<'_> =
    Deployment::new(ChainId::Mainnet, &[BATCHER_MAIN, GATEWAY_MAIN, MEMPOOL_MAIN]);
