use std::collections::BTreeMap;

use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError};

use crate::config::config_utils::create_validation_error;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Validate)]
#[validate(schema(function = "validate_monitoring_config"))]
pub struct MonitoringConfig {
    pub enable_monitoring: bool,
    pub collect_profiling_metrics: bool,
}

impl SerializeConfig for MonitoringConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "enable_monitoring",
                &self.enable_monitoring,
                "Indicating if metrics should be recorded.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "collect_profiling_metrics",
                &self.collect_profiling_metrics,
                "Indicating if profiling metrics should be collected.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self { enable_monitoring: true, collect_profiling_metrics: true }
    }
}

pub(crate) fn create_monitoring_config_validation_error() -> ValidationError {
    create_validation_error(
        "Cannot collect profiling metrics when monitoring is disabled.".to_string(),
        "Invalid monitoring configuration.",
        "Cannot collect profiling metrics when monitoring is disabled.",
    )
}

fn validate_monitoring_config(monitoring_config: &MonitoringConfig) -> Result<(), ValidationError> {
    if !monitoring_config.enable_monitoring && monitoring_config.collect_profiling_metrics {
        return Err(create_monitoring_config_validation_error());
    }
    Ok(())
}
