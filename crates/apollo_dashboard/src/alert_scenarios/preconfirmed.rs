use apollo_batcher::metrics::PRECONFIRMED_BLOCK_WRITTEN;
use apollo_metrics::metrics::MetricQueryName;

use crate::alerts::{
    Alert,
    AlertComparisonOp,
    AlertCondition,
    AlertGroup,
    AlertLogicalOp,
    AlertSeverity,
    ObserverApplicability,
    EVALUATION_INTERVAL_SEC_DEFAULT,
    PENDING_DURATION_DEFAULT,
};

/// No preconfirmed block was written in the last 10 minutes.
fn get_preconfirmed_block_not_written(alert_severity: AlertSeverity) -> Alert {
    Alert::new(
        "preconfirmed_block_not_written",
        "Preconfirmed block not written",
        AlertGroup::Batcher,
        format!("increase({}[2m])", PRECONFIRMED_BLOCK_WRITTEN.get_name_with_filter()),
        vec![AlertCondition::new(AlertComparisonOp::LessThan, 1.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        ObserverApplicability::NotApplicable,
    )
}

pub(crate) fn get_preconfirmed_block_not_written_vec() -> Vec<Alert> {
    vec![get_preconfirmed_block_not_written(AlertSeverity::Sos)]
}
