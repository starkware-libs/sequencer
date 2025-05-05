use std::collections::BTreeMap;

use apollo_config::dumping::{append_sub_config_name, ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use apollo_consensus::config::{ConsensusConfig, StreamHandlerConfig};
use apollo_consensus_orchestrator::cende::CendeConfig;
use apollo_consensus_orchestrator::config::ContextConfig;
use apollo_l1_gas_price::eth_to_strk_oracle::EthToStrkOracleConfig;
use apollo_network::NetworkConfig;
use apollo_reverts::RevertConfig;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use validator::Validate;

/// The consensus manager related configuration.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct ConsensusManagerConfig {
    pub consensus_config: ConsensusConfig,
    pub context_config: ContextConfig,
    pub eth_to_strk_oracle_config: EthToStrkOracleConfig,
    pub stream_handler_config: StreamHandlerConfig,
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
        config.extend(append_sub_config_name(
            self.eth_to_strk_oracle_config.dump(),
            "eth_to_strk_oracle_config",
        ));
        config.extend(append_sub_config_name(
            self.stream_handler_config.dump(),
            "stream_handler_config",
        ));
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
            eth_to_strk_oracle_config: EthToStrkOracleConfig::default(),
            stream_handler_config: StreamHandlerConfig::default(),
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
