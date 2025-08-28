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

use crate::types::ValidatorId;

/// Configuration for consensus.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Validate)]
pub struct ConsensusConfig {
    /// The validator ID of the node.
    pub validator_id: ValidatorId,
    /// The delay (seconds) before starting consensus to give time for network peering.
    #[serde(deserialize_with = "deserialize_seconds_to_duration")]
    pub startup_delay: Duration,
    /// Timeouts configuration for consensus.
    pub timeouts: TimeoutsConfig,
    /// The duration (seconds) between sync attempts.
    #[serde(deserialize_with = "deserialize_float_seconds_to_duration")]
    pub sync_retry_interval: Duration,
    /// Future message limits configuration.
    pub future_msg_limit: FutureMsgLimitsConfig,
}

impl SerializeConfig for ConsensusConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut config = BTreeMap::from_iter([
            ser_param(
                "validator_id",
                &self.validator_id,
                "The validator id of the node.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "startup_delay",
                &self.startup_delay.as_secs(),
                "Delay (seconds) before starting consensus to give time for network peering.",
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
        config.extend(prepend_sub_config_name(self.future_msg_limit.dump(), "future_msg_limit"));
        config
    }
}

impl Default for ConsensusConfig {
    fn default() -> Self {
        Self {
            validator_id: ValidatorId::from(DEFAULT_VALIDATOR_ID),
            startup_delay: Duration::from_secs(5),
            timeouts: TimeoutsConfig::default(),
            sync_retry_interval: Duration::from_secs_f64(1.0),
            future_msg_limit: FutureMsgLimitsConfig::default(),
        }
    }
}

/// Configuration for consensus timeouts.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct TimeoutsConfig {
    /// The timeout for a proposal.
    #[serde(deserialize_with = "deserialize_float_seconds_to_duration")]
    pub proposal_timeout: Duration,
    /// The timeout for a prevote.
    #[serde(deserialize_with = "deserialize_float_seconds_to_duration")]
    pub prevote_timeout: Duration,
    /// The timeout for a precommit.
    #[serde(deserialize_with = "deserialize_float_seconds_to_duration")]
    pub precommit_timeout: Duration,
}

impl SerializeConfig for TimeoutsConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "proposal_timeout",
                &self.proposal_timeout.as_secs_f64(),
                "The timeout (seconds) for a proposal.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "prevote_timeout",
                &self.prevote_timeout.as_secs_f64(),
                "The timeout (seconds) for a prevote.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "precommit_timeout",
                &self.precommit_timeout.as_secs_f64(),
                "The timeout (seconds) for a precommit.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

impl Default for TimeoutsConfig {
    fn default() -> Self {
        Self {
            proposal_timeout: Duration::from_secs_f64(3.0),
            prevote_timeout: Duration::from_secs_f64(1.0),
            precommit_timeout: Duration::from_secs_f64(1.0),
        }
    }
}

/// Configuration for future message limits.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Validate, PartialEq)]
pub struct FutureMsgLimitsConfig {
    /// How many heights in the future should we cache.
    pub future_height_limit: u32,
    /// How many rounds in the future (for current height) should we cache.
    pub future_round_limit: u32,
    /// How many rounds should we cache for future heights.
    pub future_height_round_limit: u32,
}

impl SerializeConfig for FutureMsgLimitsConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
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

impl Default for FutureMsgLimitsConfig {
    fn default() -> Self {
        Self { future_height_limit: 10, future_round_limit: 10, future_height_round_limit: 1 }
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
