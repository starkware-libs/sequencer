use std::collections::BTreeMap;

use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

/// The batcher related configuration.
/// TODO(Lev/Tsabary/Yael/Dafna): Define actual configuration.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct BatcherConfig {
    pub batcher_config_param_1: usize,
}

impl SerializeConfig for BatcherConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([ser_param(
            "batcher_config_param_1",
            &self.batcher_config_param_1,
            "The first batcher configuration parameter",
            ParamPrivacyInput::Public,
        )])
    }
}

impl Default for BatcherConfig {
    fn default() -> Self {
        Self { batcher_config_param_1: 1 }
    }
}
