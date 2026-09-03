use std::time::Duration;

use apollo_config::converters::{
    deserialize_float_seconds_to_duration,
    serialize_duration_as_float_seconds,
};
use apollo_config::validators::validate_ascii;
use serde::{Deserialize, Serialize};
use starknet_api::core::ChainId;
use validator::Validate;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Validate, PartialEq, Eq)]
pub struct L1EventsProviderConfig {
    #[serde(
        deserialize_with = "deserialize_float_seconds_to_duration",
        serialize_with = "serialize_duration_as_float_seconds"
    )]
    pub startup_sync_sleep_retry_interval_seconds: Duration,
    #[serde(
        deserialize_with = "deserialize_float_seconds_to_duration",
        serialize_with = "serialize_duration_as_float_seconds"
    )]
    pub l1_handler_cancellation_timelock_seconds: Duration,
    #[serde(
        deserialize_with = "deserialize_float_seconds_to_duration",
        serialize_with = "serialize_duration_as_float_seconds"
    )]
    pub l1_handler_consumption_timelock_seconds: Duration,
    #[serde(
        deserialize_with = "deserialize_float_seconds_to_duration",
        serialize_with = "serialize_duration_as_float_seconds"
    )]
    pub l1_handler_proposal_cooldown_seconds: Duration,
    /// When true, the L1 provider operates in dummy mode.
    pub dummy_mode: bool,
    /// Maximum number of commit-blocks buffered in the catch-up backlog while the provider syncs
    /// to the target height. Bounds memory growth when L2 sync stalls or lags during startup
    /// catch-up. Hitting it is a hard error rather than a silent drop, because the backlog
    /// must remain a gapless, strictly-sequential run of heights.
    pub max_commit_block_backlog_len: usize,
}

impl Default for L1EventsProviderConfig {
    fn default() -> Self {
        Self {
            startup_sync_sleep_retry_interval_seconds: Duration::from_secs(2),
            l1_handler_cancellation_timelock_seconds: Duration::from_secs(5 * 60),
            l1_handler_consumption_timelock_seconds: Duration::from_secs(5 * 60),
            l1_handler_proposal_cooldown_seconds: Duration::from_secs(70),
            dummy_mode: false,
            // ~1M entries is only ~tens of MB, comfortably covering any legitimate startup sync gap
            // while still bounding worst-case memory.
            max_commit_block_backlog_len: 1_000_000,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransactionManagerConfig {
    // How long to wait before allowing new L1 handler transactions to be proposed (validation is
    // available immediately), from the moment they are scraped.
    pub l1_handler_proposal_cooldown_seconds: Duration,
    /// How long to allow a transaction requested for cancellation to be validated against
    /// (proposals are banned upon receiving a cancellation request).
    pub l1_handler_cancellation_timelock_seconds: Duration,
    /// How long to wait before allowing a transaction that was consumed on L1 to be removed from
    /// the transaction managers records.
    // The motivation behind this timelock is to make debugging easier and to be more careful
    // about permanently deleting information.
    // This only delays a cleanup action, so the duration of the timelock wouldn't affect the UX.
    pub l1_handler_consumption_timelock_seconds: Duration,
}

impl From<L1EventsProviderConfig> for TransactionManagerConfig {
    fn from(config: L1EventsProviderConfig) -> Self {
        TransactionManagerConfig {
            l1_handler_proposal_cooldown_seconds: config.l1_handler_proposal_cooldown_seconds,
            l1_handler_cancellation_timelock_seconds: config
                .l1_handler_cancellation_timelock_seconds,
            l1_handler_consumption_timelock_seconds: config.l1_handler_consumption_timelock_seconds,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct L1EventsScraperConfig {
    #[serde(
        deserialize_with = "deserialize_float_seconds_to_duration",
        serialize_with = "serialize_duration_as_float_seconds"
    )]
    pub startup_rewind_time_seconds: Duration,
    #[validate(custom(function = "validate_ascii"))]
    pub chain_id: ChainId,
    pub finality: u64,
    #[serde(
        deserialize_with = "deserialize_float_seconds_to_duration",
        serialize_with = "serialize_duration_as_float_seconds"
    )]
    pub polling_interval_seconds: Duration,
    pub set_provider_historic_height_to_l2_genesis: bool,
    #[serde(
        deserialize_with = "deserialize_float_seconds_to_duration",
        serialize_with = "serialize_duration_as_float_seconds"
    )]
    pub l1_block_time_seconds: Duration,
    /// Maximum number of L1 blocks fetched per `events` (eth_getLogs) request. Caps the catch-up
    /// window so a large backlog is drained over successive polls instead of one unbounded
    /// request.
    #[validate(range(min = 1))]
    pub max_blocks_per_fetch: u64,
}

impl Default for L1EventsScraperConfig {
    fn default() -> Self {
        Self {
            startup_rewind_time_seconds: Duration::from_secs(60 * 60),
            chain_id: ChainId::Mainnet,
            finality: 0,
            polling_interval_seconds: Duration::from_secs(30),
            set_provider_historic_height_to_l2_genesis: false,
            l1_block_time_seconds: Duration::from_secs(12),
            // Conservative default: well under the common public-RPC eth_getLogs block-range caps
            // (~1k-10k) and the 1s base-layer timeout. Operators on permissive private RPCs may
            // raise it.
            max_blocks_per_fetch: 1000,
        }
    }
}
