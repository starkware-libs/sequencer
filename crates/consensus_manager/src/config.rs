use std::collections::BTreeMap;

use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use papyrus_consensus::config::ConsensusConfig;
use serde::{Deserialize, Serialize};
use validator::Validate;

/// The consensus manager related configuration.
/// TODO(Lev/Tsabary/Matan): Define actual configuration.
#[derive(Clone, Default, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct ConsensusManagerConfig {
    pub consensus_config: ConsensusConfig,
}

impl SerializeConfig for ConsensusManagerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([ser_param(
            "consensus_config",
            &self.consensus_config,
            "Parameters for the core consensus crate",
            ParamPrivacyInput::Public,
        )])
    }
}
