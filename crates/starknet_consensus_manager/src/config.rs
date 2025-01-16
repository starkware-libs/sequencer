use std::collections::BTreeMap;

use papyrus_config::dumping::{append_sub_config_name, ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use papyrus_consensus::config::ConsensusConfig;
use papyrus_consensus::types::ContextConfig;
use papyrus_consensus_orchestrator::cende::CendeConfig;
use papyrus_network::NetworkConfig;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
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
    pub votes_topic: String,
    pub proposals_topic: String,
    pub broadcast_buffer_size: usize,
    pub immediate_active_height: BlockNumber,
}

impl SerializeConfig for ConsensusManagerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let sub_configs = vec![
            append_sub_config_name(self.consensus_config.dump(), "consensus_config"),
            append_sub_config_name(self.context_config.dump(), "context_config"),
            append_sub_config_name(self.cende_config.dump(), "cende_config"),
            append_sub_config_name(self.network_config.dump(), "network_config"),
        ];
        let mut output_tree: BTreeMap<ParamPath, SerializedParam> =
            sub_configs.into_iter().flatten().collect();
        let additional_parameters = BTreeMap::from_iter([
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
                "The height at which the consensus manager becomes active.",
                ParamPrivacyInput::Public,
            ),
        ]);
        output_tree.extend(additional_parameters);
        output_tree
    }
}

impl Default for ConsensusManagerConfig {
    fn default() -> Self {
        ConsensusManagerConfig {
            consensus_config: ConsensusConfig::default(),
            context_config: ContextConfig::default(),
            cende_config: CendeConfig::default(),
            network_config: NetworkConfig::default(),
            votes_topic: "consensus_votes".to_string(),
            proposals_topic: "consensus_proposals".to_string(),
            broadcast_buffer_size: 100,
            immediate_active_height: BlockNumber::default(),
        }
    }
}
