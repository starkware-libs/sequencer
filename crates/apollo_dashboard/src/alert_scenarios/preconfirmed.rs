use apollo_batcher::metrics::PRECONFIRMED_BLOCK_WRITTEN;
use apollo_metrics::metrics::MetricQueryName;

use crate::alert_placeholders::SeverityValueOrPlaceholder;
use crate::alerts::{
    Alert,
    AlertComparisonOp,
    AlertCondition,
    AlertGroup,
    AlertLogicalOp,
    ObserverApplicability,
    EVALUATION_INTERVAL_SEC_DEFAULT,
    PENDING_DURATION_DEFAULT,
};

/// No preconfirmed block was written in the last 10 minutes.
fn get_preconfirmed_block_not_written(
    alert_severity: impl Into<SeverityValueOrPlaceholder>,
) -> Alert {
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
    vec![get_preconfirmed_block_not_written(SeverityValueOrPlaceholder::Placeholder(
        "preconfirmed_block_not_written".to_string(),
    ))]
}
