use std::collections::BTreeMap;

use papyrus_config::dumping::{append_sub_config_name, SerializeConfig};
use papyrus_config::{ParamPath, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_consensus::config::ConsensusConfig;
use starknet_consensus::types::ContextConfig; 
use starknet_consensus_orchestrator::cende::CendeConfig;
use validator::Validate;

/// The consensus manager related configuration.
/// TODO(Matan): Remove ConsensusManagerConfig if it's only field remains ConsensusConfig.
#[derive(Clone, Default, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct ConsensusManagerConfig {
    pub consensus_config: ConsensusConfig,
    pub context_config: ContextConfig,
    pub cende_config: CendeConfig,
}

impl SerializeConfig for ConsensusManagerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let sub_configs = vec![
            append_sub_config_name(self.consensus_config.dump(), "consensus_config"),
            append_sub_config_name(self.context_config.dump(), "context_config"),
            append_sub_config_name(self.cende_config.dump(), "cende_config"),
        ];

        sub_configs.into_iter().flatten().collect()
    }
}
