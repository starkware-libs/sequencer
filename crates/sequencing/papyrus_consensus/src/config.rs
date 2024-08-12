//! This module contains the configuration for consensus, including the `ConsensusConfig` struct
//! and its implementation of the `SerializeConfig` trait. The configuration includes parameters
//! such as the validator ID, the network topic of the consensus, and the starting block height.

use std::collections::BTreeMap;
use std::time::Duration;

use papyrus_config::converters::deserialize_seconds_to_duration;
use papyrus_config::dumping::{
    ser_optional_sub_config,
    ser_param,
    ser_required_param,
    SerializeConfig,
};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializationType, SerializedParam};
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
    /// Test configuration for consensus.
    pub test: Option<ConsensusTestConfig>,
}

impl SerializeConfig for ConsensusConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut config = BTreeMap::from_iter([
            ser_required_param(
                "validator_id",
                SerializationType::String,
                "The validator id of the node.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "network_topic",
                &self.network_topic,
                "The network topic of the consensus",
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
        config.extend(ser_optional_sub_config(&self.test, "test"));
        config
    }
}

impl Default for ConsensusConfig {
    fn default() -> Self {
        Self {
            validator_id: ValidatorId::default(),
            network_topic: "consensus".to_string(),
            start_height: BlockNumber::default(),
            num_validators: 4,
            consensus_delay: Duration::from_secs(5),
            test: None,
        }
    }
}

/// Test configuration for consensus.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct ConsensusTestConfig {
    /// The cache size for the test simulation.
    pub cache_size: usize,
    /// The random seed for the test simulation to ensure repeatable test results.
    pub random_seed: u64,
    /// The probability of dropping a message.
    pub drop_probability: f64,
    /// The probability of sending an invalid message.
    pub invalid_probability: f64,
}

impl SerializeConfig for ConsensusTestConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "cache_size",
                &self.cache_size,
                "The cache size for the test simulation.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "random_seed",
                &self.random_seed,
                "The random seed for the test simulation to ensure repeatable test results.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "drop_probability",
                &self.drop_probability,
                "The probability of dropping a message.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "invalid_probability",
                &self.invalid_probability,
                "The probability of sending an invalid message.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

impl Default for ConsensusTestConfig {
    fn default() -> Self {
        Self { cache_size: 1000, random_seed: 0, drop_probability: 0.0, invalid_probability: 0.0 }
    }
}
