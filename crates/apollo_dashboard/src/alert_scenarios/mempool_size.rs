use apollo_mempool::metrics::{
    DropReason,
    LABEL_NAME_DROP_REASON,
    MEMPOOL_POOL_SIZE,
    MEMPOOL_TRANSACTIONS_DROPPED,
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
};

fn get_mempool_pool_size_increase(
    alert_env_filtering: AlertEnvFiltering,
    alert_severity: AlertSeverity,
) -> Alert {
    Alert::new(
        "mempool_pool_size_increase",
        "Mempool pool size increase",
        AlertGroup::Mempool,
        MEMPOOL_POOL_SIZE.get_name_with_filter().to_string(),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 2000.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        alert_env_filtering,
    )
}

pub(crate) fn get_mempool_pool_size_increase_vec() -> Vec<Alert> {
    vec![
        get_mempool_pool_size_increase(
            AlertEnvFiltering::MainnetStyleAlerts,
            AlertSeverity::Regular,
        ),
        get_mempool_pool_size_increase(
            AlertEnvFiltering::TestnetStyleAlerts,
            AlertSeverity::WorkingHours,
        ),
    ]
}

fn get_mempool_evictions_count_alert(
    alert_env_filtering: AlertEnvFiltering,
    alert_severity: AlertSeverity,
) -> Alert {
    let evicted_label: &str = DropReason::Evicted.into();
    let metric_name_with_filter_and_reason = format!(
        "{}, {LABEL_NAME_DROP_REASON}=\"{evicted_label}\"}}",
        MEMPOOL_TRANSACTIONS_DROPPED
            .get_name_with_filter()
            .strip_suffix("}")
            .expect("Metric label filter should end with a }")
    );

    Alert::new(
        "mempool_evictions_count",
        "Mempool evictions count",
        AlertGroup::Mempool,
        metric_name_with_filter_and_reason,
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 0.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        alert_env_filtering,
    )
}

pub(crate) fn get_mempool_evictions_count_alert_vec() -> Vec<Alert> {
    vec![
        get_mempool_evictions_count_alert(
            AlertEnvFiltering::MainnetStyleAlerts,
            AlertSeverity::Regular,
        ),
        get_mempool_evictions_count_alert(
            AlertEnvFiltering::TestnetStyleAlerts,
            AlertSeverity::DayOnly,
        ),
    ]
}
