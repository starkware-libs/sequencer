use apollo_consensus_config::config::{ConsensusConfig, StreamHandlerConfig};
use apollo_consensus_orchestrator_config::config::{CendeConfig, ContextConfig};
use apollo_network::NetworkConfig;
use apollo_reverts::RevertConfig;
use apollo_staking_config::config::StakingManagerConfig;
use serde::{Deserialize, Serialize};
use validator::Validate;

/// The consensus manager related configuration.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct ConsensusManagerConfig {
    pub consensus_manager_config: ConsensusConfig,
    #[validate(nested)]
    pub context_config: ContextConfig,
    pub stream_handler_config: StreamHandlerConfig,
    #[validate(nested)]
    pub network_config: NetworkConfig,
    pub cende_config: CendeConfig,
    pub revert_config: RevertConfig,
    pub staking_manager_config: StakingManagerConfig,
    pub votes_topic: String,
    pub proposals_topic: String,
    pub broadcast_buffer_size: usize,
    // Assumes all validators are honest. If true, uses 1/2 votes to get quorum. Use with caution!
    pub assume_no_malicious_validators: bool,
}

impl Default for ConsensusManagerConfig {
    fn default() -> Self {
        ConsensusManagerConfig {
            consensus_manager_config: ConsensusConfig::default(),
            context_config: ContextConfig::default(),
            stream_handler_config: StreamHandlerConfig::default(),
            cende_config: CendeConfig::default(),
            network_config: NetworkConfig::default(),
            revert_config: RevertConfig::default(),
            staking_manager_config: StakingManagerConfig::default(),
            votes_topic: "consensus_votes".to_string(),
            proposals_topic: "consensus_proposals".to_string(),
            broadcast_buffer_size: 10000,
            assume_no_malicious_validators: false,
        }
    }
}
