use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Validate)]
pub struct ConfigManagerConfig {
    pub enable_config_updates: bool,
    pub config_update_interval_secs: f64,
}

impl Default for ConfigManagerConfig {
    fn default() -> Self {
        Self { enable_config_updates: false, config_update_interval_secs: 60.0 }
    }
}

#[cfg(any(feature = "testing", test))]
impl ConfigManagerConfig {
    pub fn disabled() -> Self {
        Self { enable_config_updates: false, ..Default::default() }
    }
}
