use apollo_mempool::metrics::{
    DropReason,
    LABEL_NAME_DROP_REASON,
    MEMPOOL_POOL_SIZE,
    MEMPOOL_TRANSACTIONS_DROPPED,
};
use apollo_metrics::metrics::MetricQueryName;

use crate::alert_placeholders::SeverityValueOrPlaceholder;
use crate::alerts::{
    Alert,
    AlertComparisonOp,
    AlertCondition,
    AlertLogicalOp,
    EvaluationRate,
    ObserverApplicability,
    PENDING_DURATION_DEFAULT,
};

pub(crate) fn get_mempool_pool_size_increase() -> Alert {
    const ALERT_NAME: &str = "mempool_pool_size_increase";
    Alert::new(
        ALERT_NAME,
        "Mempool pool size increase",
        EvaluationRate::Default,
        MEMPOOL_POOL_SIZE.get_name_with_filter().to_string(),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 10000.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        SeverityValueOrPlaceholder::Placeholder(ALERT_NAME.to_string()),
        ObserverApplicability::NotApplicable,
    )
}

pub(crate) fn get_mempool_evictions_count_alert() -> Alert {
    const ALERT_NAME: &str = "mempool_evictions_count";
    let evicted_label: &str = DropReason::Evicted.into();

    let query_expr = MEMPOOL_TRANSACTIONS_DROPPED.get_name_with_filer_and_additional_fields(
        &format!("{LABEL_NAME_DROP_REASON}=\"{evicted_label}\""),
    );

    Alert::new(
        ALERT_NAME,
        "Mempool evictions count",
        EvaluationRate::Default,
        query_expr,
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 0.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        SeverityValueOrPlaceholder::Placeholder(ALERT_NAME.to_string()),
        ObserverApplicability::NotApplicable,
    )
}
