use std::time::Duration;

use apollo_config::converters::{
    deserialize_milliseconds_to_duration,
    serialize_duration_as_milliseconds,
};
use apollo_network::NetworkConfig;
use serde::{Deserialize, Serialize};
use validator::Validate;

const MEMPOOL_PORT: u16 = 11111;

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Validate)]
pub struct MempoolP2pConfig {
    #[validate(nested)]
    pub network_config: NetworkConfig,
    pub network_buffer_size: usize,
    pub max_transaction_batch_size: usize,
    #[serde(
        deserialize_with = "deserialize_milliseconds_to_duration",
        serialize_with = "serialize_duration_as_milliseconds"
    )]
    pub transaction_batch_rate_millis: Duration,
    pub max_concurrent_gateway_requests: usize,
}

impl Default for MempoolP2pConfig {
    fn default() -> Self {
        Self {
            network_config: NetworkConfig { port: MEMPOOL_PORT, ..Default::default() },
            network_buffer_size: 10000,
            // TODO(Eitan): Change to appropriate values.
            max_transaction_batch_size: 1,
            transaction_batch_rate_millis: Duration::from_secs(1),
            max_concurrent_gateway_requests: 10000,
        }
    }
}
