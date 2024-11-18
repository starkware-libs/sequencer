use std::collections::BTreeMap;
use std::time::Duration;

use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

/// The http server connection related configuration.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct L1ProviderStarterConfig {
    pub interval: Duration,
}

impl SerializeConfig for L1ProviderStarterConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([ser_param(
            "interval",
            &self.interval.as_secs().to_string(),
            "The is loop interval.",
            ParamPrivacyInput::Public,
        )])
    }
}

impl Default for L1ProviderStarterConfig {
    fn default() -> Self {
        Self { interval: Duration::from_secs(10) }
    }
}
