//! This module contains the configuration for consensus, including the `ConsensusConfig` struct
//! and its implementation of the `SerializeConfig` trait. The configuration includes parameters
//! such as the validator ID, the network topic of the consensus, and the starting block height.

use std::collections::BTreeMap;
use std::time::Duration;

use papyrus_config::converters::{
    deserialize_float_seconds_to_duration,
    deserialize_seconds_to_duration,
};
use papyrus_config::dumping::{append_sub_config_name, ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;

use super::types::ValidatorId;

/// Configuration for consensus.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct ConsensusConfig {
    /// The validator ID of the node.
    pub validator_id: ValidatorId,
    /// The network topic of the consensus.
    pub network_topic: String,
    /// The height to start the consensus from.
    pub start_height: BlockNumber,
    /// The number of validators in the consensus.
    // Used for testing in an early milestones.
    pub num_validators: u64,
    /// The delay (seconds) before starting consensus to give time for network peering.
    #[serde(deserialize_with = "deserialize_seconds_to_duration")]
    pub consensus_delay: Duration,
    /// Timeouts configuration for consensus.
    pub timeouts: TimeoutsConfig,
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
                "network_topic",
                &self.network_topic,
                "The network topic of the consensus.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "start_height",
                &self.start_height,
                "The height to start the consensus from.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "num_validators",
                &self.num_validators,
                "The number of validators in the consensus.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "consensus_delay",
                &self.consensus_delay.as_secs(),
                "Delay (seconds) before starting consensus to give time for network peering.",
                ParamPrivacyInput::Public,
            ),
        ]);
        config.extend(append_sub_config_name(self.timeouts.dump(), "timeouts"));
        config
    }
}

impl Default for ConsensusConfig {
    fn default() -> Self {
        Self {
            validator_id: ValidatorId::default(),
            network_topic: "consensus".to_string(),
            start_height: BlockNumber::default(),
            num_validators: 1,
            consensus_delay: Duration::from_secs(5),
            timeouts: TimeoutsConfig::default(),
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
