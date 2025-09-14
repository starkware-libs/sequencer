use std::collections::BTreeMap;
use std::time::Duration;

use apollo_config::converters::deserialize_milliseconds_to_duration;
use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Validate)]
pub struct P2pSyncClientConfig {
    pub num_headers_per_query: u64,
    pub num_block_state_diffs_per_query: u64,
    pub num_block_transactions_per_query: u64,
    pub num_block_classes_per_query: u64,
    #[serde(deserialize_with = "deserialize_milliseconds_to_duration")]
    pub wait_period_for_new_data: Duration,
    #[serde(deserialize_with = "deserialize_milliseconds_to_duration")]
    pub wait_period_for_other_protocol: Duration,
    pub buffer_size: usize,
}

impl SerializeConfig for P2pSyncClientConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "num_headers_per_query",
                &self.num_headers_per_query,
                "The maximum amount of headers to ask from peers in each iteration.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "num_block_state_diffs_per_query",
                &self.num_block_state_diffs_per_query,
                "The maximum amount of block's state diffs to ask from peers in each iteration.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "num_block_transactions_per_query",
                &self.num_block_transactions_per_query,
                "The maximum amount of blocks to ask their transactions from peers in each \
                 iteration.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "num_block_classes_per_query",
                &self.num_block_classes_per_query,
                "The maximum amount of block's classes to ask from peers in each iteration.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "wait_period_for_new_data",
                &self.wait_period_for_new_data.as_millis(),
                "Time in millisseconds to wait when a query returned with partial data before \
                 sending a new query",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "wait_period_for_other_protocol",
                &self.wait_period_for_other_protocol.as_millis(),
                "Time in millisseconds to wait for a dependency protocol to advance (e.g.state \
                 diff sync depends on header sync)",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "buffer_size",
                &self.buffer_size,
                "Size of the buffer for read from the storage and for incoming responses.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
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
