use std::collections::BTreeMap;

use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum ActiveComponentMode {
    Disabled,
    Enabled,
}

/// The single component configuration.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct ActiveComponentExecutionConfig {
    pub execution_mode: ActiveComponentMode,
}

impl SerializeConfig for ActiveComponentExecutionConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([ser_param(
            "execution_mode",
            &self.execution_mode,
            "The component execution mode.",
            ParamPrivacyInput::Public,
        )])
    }
}

impl Default for ActiveComponentExecutionConfig {
    fn default() -> Self {
        Self { execution_mode: ActiveComponentMode::Enabled }
    }
}
