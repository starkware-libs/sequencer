use std::time::Duration;

use apollo_state_sync_metrics::metrics::{
    CENTRAL_SYNC_CENTRAL_BLOCK_MARKER,
    STATE_SYNC_CLASS_MANAGER_MARKER,
};

use crate::alerts::{
    Alert,
    AlertComparisonOp,
    AlertCondition,
    AlertEnvFiltering,
    AlertGroup,
    AlertLogicalOp,
    AlertSeverity,
    EVALUATION_INTERVAL_SEC_DEFAULT,
    PENDING_DURATION_DEFAULT,
    SECS_IN_MIN,
};

fn get_state_sync_lag(
    alert_env_filtering: AlertEnvFiltering,
    alert_severity: AlertSeverity,
) -> Alert {
    Alert::new(
        "state_sync_lag",
        "State sync lag",
        AlertGroup::StateSync,
        format!(
            "{} - {}",
            CENTRAL_SYNC_CENTRAL_BLOCK_MARKER.get_name_with_filter(),
            STATE_SYNC_CLASS_MANAGER_MARKER.get_name_with_filter()
        ), // Alert when the central sync is ahead of the class manager by more than 5 blocks
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 5.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        alert_env_filtering,
    )
}

pub(crate) fn get_state_sync_lag_vec() -> Vec<Alert> {
    vec![
        get_state_sync_lag(AlertEnvFiltering::MainnetStyleAlerts, AlertSeverity::Regular),
        get_state_sync_lag(AlertEnvFiltering::TestnetStyleAlerts, AlertSeverity::DayOnly),
    ]
}

fn get_state_sync_stuck(
    alert_name: &'static str,
    alert_env_filtering: AlertEnvFiltering,
    duration: Duration,
    alert_severity: AlertSeverity,
) -> Alert {
    Alert::new(
        alert_name,
        "State sync stuck",
        AlertGroup::StateSync,
        format!(
            "increase({}[{}s])",
            STATE_SYNC_CLASS_MANAGER_MARKER.get_name_with_filter(),
            duration.as_secs()
        ), // Alert is triggered when the class manager marker is not updated for {duration}s
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 1.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        alert_env_filtering,
    )
}

pub(crate) fn get_state_sync_stuck_vec() -> Vec<Alert> {
    vec![
        get_state_sync_stuck(
            "state_sync_stuck",
            AlertEnvFiltering::MainnetStyleAlerts,
            Duration::from_secs(2 * SECS_IN_MIN),
            AlertSeverity::Regular,
        ),
        get_state_sync_stuck(
            "state_sync_stuck",
            AlertEnvFiltering::TestnetStyleAlerts,
            Duration::from_secs(2 * SECS_IN_MIN),
            AlertSeverity::DayOnly,
        ),
        get_state_sync_stuck(
            "state_sync_stuck_long_time",
            AlertEnvFiltering::TestnetStyleAlerts,
            Duration::from_secs(30 * SECS_IN_MIN),
            AlertSeverity::Regular,
        ),
    ]
}
