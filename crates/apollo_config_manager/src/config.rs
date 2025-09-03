use std::collections::BTreeMap;
use std::path::PathBuf;

use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Validate)]
pub struct ConfigManagerConfig {
    /// Path to the dynamic configuration file that the ConfigManager should monitor and read from.
    pub config_file_path: PathBuf,
}

impl Default for ConfigManagerConfig {
    fn default() -> Self {
        Self { config_file_path: PathBuf::from("/app/config/config.json") }
    }
}

impl SerializeConfig for ConfigManagerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([ser_param(
            "config_file_path",
            &self.config_file_path,
            "Path to the dynamic configuration file.",
            ParamPrivacyInput::Public,
        )])
    }
}
