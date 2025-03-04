use starknet_api::core::ChainId;

use crate::deployment::{Deployment, Replicas, Service, ServiceName};

// TODO(Tsabary): fill and order these.

pub const MAIN_DEPLOYMENT_PRESET_PATH: &str = "config/sequencer/presets/main.json";

const BATCHER_MAIN: Service =
    Service::new(ServiceName::Batcher, "", false, Replicas::Single, Some(500));

const GATEWAY_MAIN: Service =
    Service::new(ServiceName::Gateway, "", false, Replicas::Multiple, None);

const MEMPOOL_MAIN: Service = Service::new(ServiceName::Mempool, "", false, Replicas::Single, None);

pub const MAIN_DEPLOYMENT: Deployment<'_> =
    Deployment::new(ChainId::Mainnet, &[BATCHER_MAIN, GATEWAY_MAIN, MEMPOOL_MAIN]);
