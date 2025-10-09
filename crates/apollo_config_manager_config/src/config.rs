use std::collections::BTreeMap;

use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Validate)]
pub struct ConfigManagerConfig {
    pub enable_config_updates: bool,
    pub config_update_interval_secs: f64,
}

impl SerializeConfig for ConfigManagerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "enable_config_updates",
                &self.enable_config_updates,
                "Enables the resampling of the config every `config_update_interval_secs` seconds",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "config_update_interval_secs",
                &self.config_update_interval_secs,
                "Update interval in seconds for config updates",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

impl Default for ConfigManagerConfig {
    fn default() -> Self {
        Self { enable_config_updates: false, config_update_interval_secs: 60.0 }
    }
}

// TODO(Tsabary): wrap under `testing` feature.
impl ConfigManagerConfig {
    pub fn disabled() -> Self {
        Self { enable_config_updates: false, ..Default::default() }
    }
}
