use apollo_config::validators::create_validation_error;
use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Validate)]
#[validate(schema(function = "validate_monitoring_config"))]
pub struct MonitoringConfig {
    pub collect_metrics: bool,
    pub collect_profiling_metrics: bool,
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self { collect_metrics: true, collect_profiling_metrics: true }
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
    if !monitoring_config.collect_metrics && monitoring_config.collect_profiling_metrics {
        return Err(create_monitoring_config_validation_error());
    }
    Ok(())
}
