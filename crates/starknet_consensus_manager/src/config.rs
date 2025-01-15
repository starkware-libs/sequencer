use std::collections::BTreeMap;

use papyrus_config::dumping::{append_sub_config_name, ser_param,ser_optional_param, SerializeConfig};
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
    pub revert_up_to_and_including: Option<BlockNumber>,
    pub votes_topic: String,
    pub proposals_topic: String,
    pub immediate_active_height: u64,
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
                "immediate_active_height",
                &self.immediate_active_height,
                "The height at which the consensus manager becomes active.",
                ParamPrivacyInput::Public,
            )
        ]);
        config.extend(ser_optional_param(
            &self.revert_up_to_and_including,
            // Use u64::MAX as a placeholder to prevent setting this value to
            // a low block number by mistake, which will cause significant revert operations.
            BlockNumber(u64::MAX),
            "revert_up_to_and_including",
            "The batcher will revert blocks up to this block number (including). Use this \
             configurations carefully to prevent significant revert operations and data loss.",
            ParamPrivacyInput::Private,
        ));
        config.extend(append_sub_config_name(self.consensus_config.dump(), "consensus_config"));
        config.extend(append_sub_config_name(self.context_config.dump(), "context_config"));
        config.extend(append_sub_config_name(self.cende_config.dump(), "cende_config"));
        config.extend(append_sub_config_name(self.network_config.dump(), "network_config"));
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
            revert_up_to_and_including: None,
            votes_topic: "consensus_votes".to_string(),
            proposals_topic: "consensus_proposals".to_string(),
            immediate_active_height: 0,
        }
    }
}
