use std::time::Duration;

use apollo_metrics::metrics::MetricQueryName;
use apollo_state_sync_metrics::metrics::{
    CENTRAL_SYNC_CENTRAL_BLOCK_MARKER,
    STATE_SYNC_CLASS_MANAGER_MARKER,
};

use crate::alert_placeholders::SeverityValueOrPlaceholder;
use crate::alerts::{
    Alert,
    AlertComparisonOp,
    AlertCondition,
    EvaluationRate,
    AlertLogicalOp,
    AlertSeverity,
    ObserverApplicability,
    EVALUATION_INTERVAL_SEC_DEFAULT,
    PENDING_DURATION_DEFAULT,
    SECS_IN_MIN,
};

pub(crate) fn get_state_sync_lag() -> Alert {
    const ALERT_NAME: &str = "state_sync_lag";
    Alert::new(
        ALERT_NAME,
        "State sync lag",
        EvaluationRate::Default,
        format!(
            "{} - {}",
            CENTRAL_SYNC_CENTRAL_BLOCK_MARKER.get_name_with_filter(),
            STATE_SYNC_CLASS_MANAGER_MARKER.get_name_with_filter()
        ), // Alert when the central sync is ahead of the class manager by more than 5 blocks
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 5.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        SeverityValueOrPlaceholder::Placeholder(ALERT_NAME.to_string()),
        ObserverApplicability::NotApplicable,
    )
}

fn get_state_sync_stuck(
    alert_name: &'static str,
    duration: Duration,
    alert_severity: AlertSeverity,
) -> Alert {
    Alert::new(
        alert_name.to_lowercase().replace(' ', "_"),
        alert_name,
        EvaluationRate::Default,
        format!(
            "increase({}[{}s])",
            STATE_SYNC_CLASS_MANAGER_MARKER.get_name_with_filter(),
            duration.as_secs()
        ), // Alert is triggered when the class manager marker is not updated for {duration}s
        vec![AlertCondition::new(AlertComparisonOp::LessThan, 1.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        ObserverApplicability::Applicable,
    )
}

pub(crate) fn get_state_sync_stuck_vec() -> Vec<Alert> {
    vec![
        get_state_sync_stuck(
            "State Sync Stuck",
            Duration::from_secs(2 * SECS_IN_MIN),
            AlertSeverity::Regular,
        ),
        get_state_sync_stuck(
            "State Sync Stuck Long Time",
            Duration::from_secs(30 * SECS_IN_MIN),
            AlertSeverity::Regular,
        ),
    ]
}
