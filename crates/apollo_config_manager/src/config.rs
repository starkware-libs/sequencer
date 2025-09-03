use std::collections::BTreeMap;

use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Deserialize, Serialize, Clone, Default, PartialEq, Validate)]
pub struct ConfigManagerConfig {
    #[serde(default = "default_config_path")]
    pub config_path: String,
}

fn default_config_path() -> String {
    // TODO(Nadin): Get the value from actual configuration source instead of hardcoded default
    "/config/sequencer/presets/config".to_string()
}

impl SerializeConfig for ConfigManagerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([ser_param(
            "config_path",
            &self.config_path,
            "Path to the configuration directory.",
            ParamPrivacyInput::Public,
        )])
    }
}
