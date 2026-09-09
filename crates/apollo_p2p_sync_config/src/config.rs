use std::time::Duration;

use apollo_config::converters::{
    deserialize_milliseconds_to_duration,
    serialize_duration_as_milliseconds,
};
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Validate)]
pub struct P2pSyncClientConfig {
    pub num_headers_per_query: u64,
    pub num_block_state_diffs_per_query: u64,
    pub num_block_transactions_per_query: u64,
    pub num_block_classes_per_query: u64,
    #[serde(
        deserialize_with = "deserialize_milliseconds_to_duration",
        serialize_with = "serialize_duration_as_milliseconds"
    )]
    pub wait_period_for_new_data: Duration,
    #[serde(
        deserialize_with = "deserialize_milliseconds_to_duration",
        serialize_with = "serialize_duration_as_milliseconds"
    )]
    pub wait_period_for_other_protocol: Duration,
    pub buffer_size: usize,
}

impl Default for P2pSyncClientConfig {
    fn default() -> Self {
        P2pSyncClientConfig {
            num_headers_per_query: 10000,
            // State diffs are split into multiple messages, so big queries can lead to a lot of
            // messages in the network buffers.
            num_block_state_diffs_per_query: 100,
            num_block_transactions_per_query: 100,
            num_block_classes_per_query: 100,
            wait_period_for_new_data: Duration::from_millis(50),
            wait_period_for_other_protocol: Duration::from_millis(50),
            // TODO(eitan): split this by protocol
            buffer_size: 100000,
        }
    }
}
