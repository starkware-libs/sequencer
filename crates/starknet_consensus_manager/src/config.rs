use std::collections::BTreeMap;

use apollo_reverts::RevertConfig;
use papyrus_config::dumping::{append_sub_config_name, ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use papyrus_network::NetworkConfig;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_consensus::config::ConsensusConfig;
use starknet_consensus::types::ContextConfig;
use starknet_consensus_orchestrator::cende::CendeConfig;
use validator::Validate;

/// The consensus manager related configuration.
/// TODO(Matan): Remove ConsensusManagerConfig if it's only field remains ConsensusConfig.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct ConsensusManagerConfig {
    pub consensus_config: ConsensusConfig,
    pub context_config: ContextConfig,
    #[validate]
    pub network_config: NetworkConfig,
    pub cende_config: CendeConfig,
    pub revert_config: RevertConfig,
    pub votes_topic: String,
    pub proposals_topic: String,
    pub broadcast_buffer_size: usize,
    pub immediate_active_height: BlockNumber,
}

impl SerializeConfig for ConsensusManagerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut config = BTreeMap::from_iter([
            ser_param(
                "votes_topic",
                &self.votes_topic,
                "The topic for consensus votes.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "proposals_topic",
                &self.proposals_topic,
                "The topic for consensus proposals.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "broadcast_buffer_size",
                &self.broadcast_buffer_size,
                "The buffer size for the broadcast channel.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "immediate_active_height",
                &self.immediate_active_height,
                "The height at which the node may actively participate in consensus.",
                ParamPrivacyInput::Public,
            ),
        ]);
        config.extend(append_sub_config_name(self.consensus_config.dump(), "consensus_config"));
        config.extend(append_sub_config_name(self.context_config.dump(), "context_config"));
        config.extend(append_sub_config_name(self.cende_config.dump(), "cende_config"));
        config.extend(append_sub_config_name(self.network_config.dump(), "network_config"));
        config.extend(append_sub_config_name(self.revert_config.dump(), "revert_config"));
        config
    }
}

impl Default for ConsensusManagerConfig {
    fn default() -> Self {
        ConsensusManagerConfig {
            consensus_config: ConsensusConfig::default(),
            context_config: ContextConfig::default(),
            cende_config: CendeConfig::default(),
            network_config: NetworkConfig::default(),
            revert_config: RevertConfig::default(),
            votes_topic: "consensus_votes".to_string(),
            proposals_topic: "consensus_proposals".to_string(),
            broadcast_buffer_size: 10000,
            immediate_active_height: BlockNumber::default(),
        }
    }
}
