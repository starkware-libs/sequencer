use std::collections::BTreeMap;

use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Deserialize, Default, Serialize, Clone, PartialEq, Validate)]
pub struct ConfigManagerConfig {
    /// Placeholder field - config cannot be empty for proper deserialization
    pub _placeholder: String,
}

impl SerializeConfig for ConfigManagerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([ser_param(
            "_placeholder",
            &self._placeholder,
            "Placeholder field - config cannot be empty for proper deserialization",
            ParamPrivacyInput::Public,
        )])
    }
}
