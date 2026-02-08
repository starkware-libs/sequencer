use apollo_mempool::metrics::{
    DropReason,
    LABEL_NAME_DROP_REASON,
    MEMPOOL_POOL_SIZE,
    MEMPOOL_TRANSACTIONS_DROPPED,
};
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

fn get_mempool_pool_size_increase(alert_severity: AlertSeverity) -> Alert {
    Alert::new(
        "mempool_pool_size_increase",
        "Mempool pool size increase",
        AlertGroup::Mempool,
        MEMPOOL_POOL_SIZE.get_name_with_filter().to_string(),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 10000.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        ObserverApplicability::NotApplicable,
    )
}

pub(crate) fn get_mempool_pool_size_increase_vec() -> Vec<Alert> {
    vec![get_mempool_pool_size_increase(AlertSeverity::DayOnly)]
}

fn get_mempool_evictions_count_alert(alert_severity: AlertSeverity) -> Alert {
    let evicted_label: &str = DropReason::Evicted.into();

    let query_expr = MEMPOOL_TRANSACTIONS_DROPPED.get_name_with_filer_and_additional_fields(
        &format!("{LABEL_NAME_DROP_REASON}=\"{evicted_label}\""),
    );

    Alert::new(
        "mempool_evictions_count",
        "Mempool evictions count",
        AlertGroup::Mempool,
        query_expr,
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 0.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        ObserverApplicability::NotApplicable,
    )
}

pub(crate) fn get_mempool_evictions_count_alert_vec() -> Vec<Alert> {
    vec![get_mempool_evictions_count_alert(AlertSeverity::Regular)]
}
