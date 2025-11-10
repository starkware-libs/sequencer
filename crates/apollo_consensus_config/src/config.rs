//! This module contains the configuration for consensus, including the `ConsensusConfig` struct
//! and its implementation of the `SerializeConfig` trait. The configuration includes parameters
//! such as the validator ID, the network topic of the consensus, and the starting block height.

use std::collections::BTreeMap;
use std::time::Duration;

use apollo_config::converters::{
    deserialize_float_seconds_to_duration,
    deserialize_seconds_to_duration,
};
use apollo_config::dumping::{prepend_sub_config_name, ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use apollo_protobuf::consensus::DEFAULT_VALIDATOR_ID;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::ValidatorId;

/// Dynamic configuration for consensus that can change at runtime.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Validate)]
pub struct ConsensusDynamicConfig {
    /// The validator ID of the node.
    pub validator_id: ValidatorId,
    /// Timeouts configuration for consensus.
    pub timeouts: TimeoutsConfig,
    /// The duration (seconds) between sync attempts.
    #[serde(deserialize_with = "deserialize_float_seconds_to_duration")]
    pub sync_retry_interval: Duration,
}

/// Static configuration for consensus that doesn't change during runtime.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Validate)]
pub struct ConsensusStaticConfig {
    /// The delay (seconds) before starting consensus to give time for network peering.
    #[serde(deserialize_with = "deserialize_seconds_to_duration")]
    pub startup_delay: Duration,
    /// How many heights in the future should we cache.
    pub future_height_limit: u32,
    /// How many rounds in the future (for current height) should we cache.
    pub future_round_limit: u32,
    /// How many rounds should we cache for future heights.
    pub future_height_round_limit: u32,
}

/// Configuration for consensus containing both static and dynamic configs.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Validate)]
pub struct ConsensusConfig {
    #[validate]
    pub dynamic_config: ConsensusDynamicConfig,
    #[validate]
    pub static_config: ConsensusStaticConfig,
}

impl SerializeConfig for ConsensusDynamicConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut config = BTreeMap::from_iter([
            ser_param(
                "validator_id",
                &self.validator_id,
                "The validator id of the node.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "sync_retry_interval",
                &self.sync_retry_interval.as_secs_f64(),
                "The duration (seconds) between sync attempts.",
                ParamPrivacyInput::Public,
            ),
        ]);
        config.extend(prepend_sub_config_name(self.timeouts.dump(), "timeouts"));
        config
    }
}

impl SerializeConfig for ConsensusStaticConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "startup_delay",
                &self.startup_delay.as_secs(),
                "Delay (seconds) before starting consensus to give time for network peering.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "future_height_limit",
                &self.future_height_limit,
                "How many heights in the future should we cache.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "future_round_limit",
                &self.future_round_limit,
                "How many rounds in the future (for current height) should we cache.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "future_height_round_limit",
                &self.future_height_round_limit,
                "How many rounds should we cache for future heights.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

impl SerializeConfig for ConsensusConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut config = BTreeMap::new();
        config.extend(prepend_sub_config_name(self.dynamic_config.dump(), "dynamic_config"));
        config.extend(prepend_sub_config_name(self.static_config.dump(), "static_config"));
        config
    }
}

impl Default for ConsensusDynamicConfig {
    fn default() -> Self {
        Self {
            validator_id: ValidatorId::from(DEFAULT_VALIDATOR_ID),
            timeouts: TimeoutsConfig::default(),
            sync_retry_interval: Duration::from_secs_f64(1.0),
        }
    }
}

impl Default for ConsensusStaticConfig {
    fn default() -> Self {
        Self {
            startup_delay: Duration::from_secs(5),
            future_height_limit: 10,
            future_round_limit: 10,
            future_height_round_limit: 1,
        }
    }
}

impl ConsensusConfig {
    // TODO(Nadin): create a generic trait for this
    pub fn from_parts(
        dynamic_config: ConsensusDynamicConfig,
        static_config: ConsensusStaticConfig,
    ) -> Self {
        Self { dynamic_config, static_config }
    }
}

impl Default for ConsensusConfig {
    fn default() -> Self {
        Self::from_parts(ConsensusDynamicConfig::default(), ConsensusStaticConfig::default())
    }
}

/// A single timeout definition with base, per-round delta, and a maximum duration.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct Timeout {
    /// The base timeout (seconds).
    #[serde(deserialize_with = "deserialize_float_seconds_to_duration")]
    pub base: Duration,
    /// The per-round delta added to the timeout (seconds).
    #[serde(deserialize_with = "deserialize_float_seconds_to_duration")]
    pub delta: Duration,
    /// The maximum timeout duration (seconds).
    #[serde(deserialize_with = "deserialize_float_seconds_to_duration")]
    pub max: Duration,
}

impl Timeout {
    /// Compute the timeout for the given round: min(base + round * delta, max).
    pub fn get_timeout(&self, round: u32) -> Duration {
        self.base.saturating_add(self.delta.saturating_mul(round)).min(self.max)
    }
}

/// Configuration for consensus timeouts.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct TimeoutsConfig {
    /// Proposal timeout configuration.
    pub proposal: Timeout,
    /// Prevote timeout configuration.
    pub prevote: Timeout,
    /// Precommit timeout configuration.
    pub precommit: Timeout,
}

impl Default for TimeoutsConfig {
    fn default() -> Self {
        Self {
            proposal: Timeout {
                base: Duration::from_secs_f64(3.0),
                delta: Duration::from_secs_f64(2.0),
                max: Duration::from_secs_f64(30.0),
            },
            prevote: Timeout {
                base: Duration::from_secs_f64(1.0),
                delta: Duration::from_secs_f64(0.5),
                max: Duration::from_secs_f64(5.0),
            },
            precommit: Timeout {
                base: Duration::from_secs_f64(1.0),
                delta: Duration::from_secs_f64(0.5),
                max: Duration::from_secs_f64(5.0),
            },
        }
    }
}

impl SerializeConfig for TimeoutsConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "proposal.base",
                &self.proposal.base.as_secs_f64(),
                "The timeout (seconds) for a proposal.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "proposal.delta",
                &self.proposal.delta.as_secs_f64(),
                "The per-round timeout delta (seconds) for a proposal.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "proposal.max",
                &self.proposal.max.as_secs_f64(),
                "The maximum timeout (seconds) for a proposal.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "prevote.base",
                &self.prevote.base.as_secs_f64(),
                "The timeout (seconds) for a prevote.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "prevote.delta",
                &self.prevote.delta.as_secs_f64(),
                "The per-round timeout delta (seconds) for a prevote.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "prevote.max",
                &self.prevote.max.as_secs_f64(),
                "The maximum timeout (seconds) for a prevote.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "precommit.base",
                &self.precommit.base.as_secs_f64(),
                "The timeout (seconds) for a precommit.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "precommit.delta",
                &self.precommit.delta.as_secs_f64(),
                "The per-round timeout delta (seconds) for a precommit.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "precommit.max",
                &self.precommit.max.as_secs_f64(),
                "The maximum timeout (seconds) for a precommit.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

/// Configuration for the `StreamHandler`.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct StreamHandlerConfig {
    /// The capacity of the channel buffer for stream messages.
    pub channel_buffer_capacity: usize,
    /// The maximum number of streams that can be open at the same time.
    pub max_streams: usize,
}

impl Default for StreamHandlerConfig {
    fn default() -> Self {
        Self { channel_buffer_capacity: 1000, max_streams: 100 }
    }
}

impl SerializeConfig for StreamHandlerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "channel_buffer_capacity",
                &self.channel_buffer_capacity,
                "The capacity of the channel buffer for stream messages.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_streams",
                &self.max_streams,
                "The maximum number of streams that can be open at the same time.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}
