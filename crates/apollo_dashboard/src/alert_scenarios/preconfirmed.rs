use apollo_batcher::metrics::PRECONFIRMED_BLOCK_WRITTEN;
use apollo_metrics::metrics::MetricQueryName;

use crate::alert_placeholders::SeverityValueOrPlaceholder;
use crate::alerts::{
    Alert,
    AlertComparisonOp,
    AlertCondition,
    EvaluationRate,
    AlertLogicalOp,
    ObserverApplicability,
    EVALUATION_INTERVAL_SEC_DEFAULT,
    PENDING_DURATION_DEFAULT,
};

/// No preconfirmed block was written in the last 10 minutes.
pub(crate) fn get_preconfirmed_block_not_written() -> Alert {
    const ALERT_NAME: &str = "preconfirmed_block_not_written";
    Alert::new(
        ALERT_NAME,
        "Preconfirmed block not written",
        EvaluationRate::Default,
        format!("increase({}[2m])", PRECONFIRMED_BLOCK_WRITTEN.get_name_with_filter()),
        vec![AlertCondition::new(AlertComparisonOp::LessThan, 1.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        SeverityValueOrPlaceholder::Placeholder(ALERT_NAME.to_string()),
        ObserverApplicability::NotApplicable,
    )
}
