use std::time::Duration;

use apollo_config::behavior_mode::BehaviorMode;
use apollo_config::converters::{deserialize_seconds_to_duration, serialize_duration_as_seconds};
use serde::{Deserialize, Serialize};
use url::Url;
use validator::Validate;

/// Configuration for consensus containing both static and dynamic configs.
#[derive(Debug, Deserialize, Default, Serialize, Clone, PartialEq, Validate)]
pub struct MempoolConfig {
    #[validate(nested)]
    pub dynamic_config: MempoolDynamicConfig,
    #[validate(nested)]
    pub static_config: MempoolStaticConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Validate)]
pub struct MempoolDynamicConfig {
    // Time-to-live for transactions in the mempool, in seconds.
    // Transactions older than this value will be lazily removed.
    #[serde(
        deserialize_with = "deserialize_seconds_to_duration",
        serialize_with = "serialize_duration_as_seconds"
    )]
    pub transaction_ttl: Duration,
}

impl Default for MempoolDynamicConfig {
    fn default() -> Self {
        Self {
            transaction_ttl: Duration::from_secs(60), // 1 minute.
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Validate)]
pub struct MempoolStaticConfig {
    pub enable_fee_escalation: bool,
    // TODO(AlonH): consider adding validations; should be bounded?
    // Percentage increase for tip and max gas price to enable transaction replacement.
    pub fee_escalation_percentage: u8, // E.g., 10 for a 10% increase.
    // If true, only transactions with max L2 gas price per unit bound that are above the threshold
    // are inserted into the priority queue. If false, all transactions are inserted into the
    // priority queue.
    pub validate_resource_bounds: bool,
    // Time to wait before allowing a Declare transaction to be returned in `get_txs`.
    // Declare transactions are delayed to allow other nodes sufficient time to compile them.
    #[serde(
        deserialize_with = "deserialize_seconds_to_duration",
        serialize_with = "serialize_duration_as_seconds"
    )]
    pub declare_delay: Duration,
    // Number of latest committed blocks for which committed account nonces are preserved.
    pub committed_nonce_retention_block_count: usize,
    // The maximum size of the mempool, in bytes.
    pub capacity_in_bytes: u64,
    // Determines queue type and other behavior.
    pub behavior_mode: BehaviorMode,
    // The URL of the recorder service (used for FIFO queue timestamp fetching).
    pub recorder_url: Url,
}

impl Default for MempoolStaticConfig {
    fn default() -> Self {
        Self {
            enable_fee_escalation: true,
            validate_resource_bounds: true,
            fee_escalation_percentage: 10,
            declare_delay: Duration::from_secs(1),
            committed_nonce_retention_block_count: 100,
            capacity_in_bytes: 1 << 30, // 1GB.
            behavior_mode: BehaviorMode::Starknet,
            recorder_url: "https://recorder_url"
                .parse::<Url>()
                .expect("recorder_url must be a valid Recorder URL"),
        }
    }
}
