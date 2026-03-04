use std::collections::BTreeMap;
use std::time::Duration;

use apollo_config::converters::deserialize_float_seconds_to_duration;
use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::validators::validate_ascii;
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::core::ChainId;
use validator::Validate;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Validate, PartialEq, Eq)]
pub struct L1EventsProviderConfig {
    #[serde(deserialize_with = "deserialize_float_seconds_to_duration")]
    pub startup_sync_sleep_retry_interval_seconds: Duration,
    #[serde(deserialize_with = "deserialize_float_seconds_to_duration")]
    pub l1_handler_cancellation_timelock_seconds: Duration,
    #[serde(deserialize_with = "deserialize_float_seconds_to_duration")]
    pub l1_handler_consumption_timelock_seconds: Duration,
    #[serde(deserialize_with = "deserialize_float_seconds_to_duration")]
    pub l1_handler_proposal_cooldown_seconds: Duration,
    /// When true, the L1 provider operates in dummy mode.
    pub dummy_mode: bool,
}

impl Default for L1EventsProviderConfig {
    fn default() -> Self {
        Self {
            startup_sync_sleep_retry_interval_seconds: Duration::from_secs(2),
            l1_handler_cancellation_timelock_seconds: Duration::from_secs(5 * 60),
            l1_handler_consumption_timelock_seconds: Duration::from_secs(5 * 60),
            l1_handler_proposal_cooldown_seconds: Duration::from_secs(70),
            dummy_mode: false,
        }
    }
}

impl SerializeConfig for L1EventsProviderConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([
            ser_param(
                "startup_sync_sleep_retry_interval_seconds",
                &self.startup_sync_sleep_retry_interval_seconds.as_secs(),
                "Interval in seconds between each retry of syncing with L2 during startup.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "l1_handler_cancellation_timelock_seconds",
                &self.l1_handler_cancellation_timelock_seconds.as_secs(),
                "How long to allow a transaction requested for cancellation to be validated \
                 against (proposals are banned upon receiving a cancellation request).",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "l1_handler_consumption_timelock_seconds",
                &self.l1_handler_consumption_timelock_seconds.as_secs_f64(),
                "How long to wait after a transaction is consumed on L1 before it can be cleared \
                 from the transaction manager.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "l1_handler_proposal_cooldown_seconds",
                &self.l1_handler_proposal_cooldown_seconds.as_secs(),
                "How long to wait before allowing new L1 handler transactions to be proposed \
                 (validation is available immediately), from the moment they are scraped.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "dummy_mode",
                &self.dummy_mode,
                "When true, the L1 provider operates in dummy mode, always responding with \
                 trivial truthy responses without connecting to actual L1.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct L1EventsScraperConfig {
    #[serde(deserialize_with = "deserialize_float_seconds_to_duration")]
    pub startup_rewind_time_seconds: Duration,
    #[validate(custom(function = "validate_ascii"))]
    pub chain_id: ChainId,
    pub finality: u64,
    #[serde(deserialize_with = "deserialize_float_seconds_to_duration")]
    pub polling_interval_seconds: Duration,
    pub set_provider_historic_height_to_l2_genesis: bool,
    #[serde(deserialize_with = "deserialize_float_seconds_to_duration")]
    pub l1_block_time_seconds: Duration,
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
        }
    }
}

impl SerializeConfig for L1EventsScraperConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([
            ser_param(
                "startup_rewind_time_seconds",
                &self.startup_rewind_time_seconds.as_secs(),
                "Duration in seconds to rewind from latest L1 block when starting scraping.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "finality",
                &self.finality,
                "Number of blocks to wait for finality",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "polling_interval_seconds",
                &self.polling_interval_seconds.as_secs(),
                "Interval in Seconds between each scraping attempt of L1.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "chain_id",
                &self.chain_id,
                "The chain to follow. For more details see https://docs.starknet.io/documentation/architecture_and_concepts/Blocks/transactions/#chain-id.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "set_provider_historic_height_to_l2_genesis",
                &self.set_provider_historic_height_to_l2_genesis,
                "When true, the scraper will send the provider an historic height set to the L2 genesis (height zero). \
                             This is useful on new chains (or in tests) where there have not been any state updates to the Starknet contract.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "l1_block_time_seconds",
                &self.l1_block_time_seconds.as_secs(),
                "The time it takes for a new L1 block to be created.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}
