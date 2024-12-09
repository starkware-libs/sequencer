use std::collections::BTreeMap;

use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::config::reactive_component_config::ReactiveComponentMode;

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

// There are components which are originally described with a reactive mode setting, however, result
// in the creation of two components, with the other described with an active mode setting. The
// following method enables converting a reactive mode setting to an active mode setting.
impl From<ReactiveComponentMode> for ActiveComponentMode {
    fn from(mode: ReactiveComponentMode) -> Self {
        match mode {
            ReactiveComponentMode::Disabled | ReactiveComponentMode::Remote => {
                ActiveComponentMode::Disabled
            }
            ReactiveComponentMode::LocalExecutionWithRemoteEnabled
            | ReactiveComponentMode::LocalExecutionWithRemoteDisabled => {
                ActiveComponentMode::Enabled
            }
        }
    }
}
