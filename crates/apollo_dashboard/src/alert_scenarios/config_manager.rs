use apollo_config_manager::metrics::CONFIG_MANAGER_UPDATE_ERRORS;
use apollo_metrics::metrics::MetricQueryName;

use crate::alerts::{
    Alert,
    AlertComparisonOp,
    AlertCondition,
    AlertLogicalOp,
    AlertSeverity,
    EvaluationRate,
    ObserverApplicability,
    PENDING_DURATION_DEFAULT,
};

/// Alert when the config manager reports update errors.
pub(crate) fn get_config_manager_update_error_increase() -> Alert {
    Alert::new(
        "config_manager_update_error_increase",
        "Config manager update error increase",
        EvaluationRate::Default,
        format!(
            "sum(increase({}[5m])) or vector(0)",
            CONFIG_MANAGER_UPDATE_ERRORS.get_name_with_filter()
        ),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 0.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        AlertSeverity::Regular,
        ObserverApplicability::NotApplicable,
    )
}
