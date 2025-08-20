use apollo_batcher::metrics::PRECONFIRMED_BLOCK_WRITTEN;

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
};

/// No preconfirmed block was written in the last 10 minutes.
fn get_preconfirmed_block_not_written(
    alert_env_filtering: AlertEnvFiltering,
    alert_severity: AlertSeverity,
) -> Alert {
    Alert::new(
        "preconfirmed_block_not_written",
        "Preconfirmed block not written",
        AlertGroup::Batcher,
        format!("increase({}[2m])", PRECONFIRMED_BLOCK_WRITTEN.get_name_with_filter()),
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

pub(crate) fn get_preconfirmed_block_not_written_vec() -> Vec<Alert> {
    vec![
        get_preconfirmed_block_not_written(
            AlertEnvFiltering::MainnetStyleAlerts,
            AlertSeverity::Regular,
        ),
        get_preconfirmed_block_not_written(
            AlertEnvFiltering::TestnetStyleAlerts,
            AlertSeverity::WorkingHours,
        ),
    ]
}
