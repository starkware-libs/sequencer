use std::collections::BTreeMap;

use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

/// The consensus manager related configuration.
/// TODO(Lev/Tsabary/Matan): Define actual configuration.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct ConsensusManagerConfig {
    pub consensus_config_param_1: usize,
}

impl SerializeConfig for ConsensusManagerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([ser_param(
            "consensus_config_param_1",
            &self.consensus_config_param_1,
            "The first consensus manager configuration parameter",
            ParamPrivacyInput::Public,
        )])
    }
}

impl Default for ConsensusManagerConfig {
    fn default() -> Self {
        Self { consensus_config_param_1: 1 }
    }
}
