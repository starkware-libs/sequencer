use apollo_config_manager::metrics::CONFIG_MANAGER_UPDATE_ERRORS;
use apollo_metrics::metrics::MetricQueryName;

use crate::alerts::{
    Alert,
    AlertComparisonOp,
    AlertCondition,
    AlertEnvFiltering,
    AlertGroup,
    AlertLogicalOp,
    AlertSeverity,
    ObserverApplicability,
    EVALUATION_INTERVAL_SEC_DEFAULT,
    PENDING_DURATION_DEFAULT,
};

/// Alert when the config manager reports update errors (load/validate or set dynamic config).
/// Uses default trigger timing: condition must hold for 30s before firing, evaluated every 30s.
pub(crate) fn get_config_manager_update_error_increase() -> Alert {
    Alert::new(
        "config_manager_update_error_increase",
        "Config manager update error increase",
        AlertGroup::General,
        format!(
            "sum(increase({}[5m])) or vector(0)",
            CONFIG_MANAGER_UPDATE_ERRORS.get_name_with_filter()
        ),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 0.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Regular,
        ObserverApplicability::NotApplicable,
        AlertEnvFiltering::All,
    )
}
