use apollo_http_server::metrics::{
    ADDED_TRANSACTIONS_DEPRECATED_ERROR,
    ADDED_TRANSACTIONS_FAILURE,
    ADDED_TRANSACTIONS_INTERNAL_ERROR,
    ADDED_TRANSACTIONS_TOTAL,
};
use apollo_mempool::metrics::{MEMPOOL_TRANSACTIONS_DROPPED, MEMPOOL_TRANSACTIONS_RECEIVED};
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

// TODO(guy.f): consider uniting with regular tx failure rate.
// TODO(guyf.f): Change threshold to 0.05 after mainnet launch.
pub(crate) fn get_http_server_high_deprecated_transaction_failure_ratio() -> Alert {
    Alert::new(
        "http_server_high_deprecated_transaction_failure_ratio",
        "http server high deprecated transaction failure ratio",
        AlertGroup::HttpServer,
        format!(
            "increase({}[1h]) / clamp_min(increase({}[1h]), 1)",
            ADDED_TRANSACTIONS_DEPRECATED_ERROR.get_name_with_filter(),
            ADDED_TRANSACTIONS_TOTAL.get_name_with_filter()
        ),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 0.1, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Informational,
        ObserverApplicability::NotApplicable,
    )
}

pub(crate) fn get_http_server_high_transaction_failure_ratio() -> Alert {
    Alert::new(
        "http_server_high_transaction_failure_ratio",
        "http server high transaction failure ratio",
        AlertGroup::HttpServer,
        format!(
            "(increase({}[1h]) - increase({}[1h])) / clamp_min(increase({}[1h]), 1)",
            ADDED_TRANSACTIONS_FAILURE.get_name_with_filter(),
            ADDED_TRANSACTIONS_DEPRECATED_ERROR.get_name_with_filter(),
            ADDED_TRANSACTIONS_TOTAL.get_name_with_filter()
        ),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 0.5, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Informational,
        ObserverApplicability::NotApplicable,
    )
}

fn get_http_server_internal_error_ratio(alert_severity: AlertSeverity) -> Alert {
    Alert::new(
        "http_server_internal_error_ratio",
        "http server internal error ratio",
        AlertGroup::HttpServer,
        format!(
            "increase({}[1h]) / clamp_min(increase({}[1h]), 1)",
            ADDED_TRANSACTIONS_INTERNAL_ERROR.get_name_with_filter(),
            ADDED_TRANSACTIONS_TOTAL.get_name_with_filter()
        ),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 0.01, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        ObserverApplicability::NotApplicable,
    )
}

pub(crate) fn get_http_server_internal_error_ratio_vec() -> Vec<Alert> {
    vec![get_http_server_internal_error_ratio(AlertSeverity::Regular)]
}

fn get_mempool_transaction_drop_ratio(alert_severity: AlertSeverity) -> Alert {
    Alert::new(
        "mempool_transaction_drop_ratio",
        "Mempool transaction drop ratio",
        AlertGroup::Mempool,
        format!(
            "increase({}[10m]) / clamp_min(increase({}[10m]), 1)",
            MEMPOOL_TRANSACTIONS_DROPPED.get_name_with_filter(),
            MEMPOOL_TRANSACTIONS_RECEIVED.get_name_with_filter(),
        ),
        vec![AlertCondition::new(
            AlertComparisonOp::GreaterThan,
            // TODO(leo): Decide on the final ratio and who should be alerted.
            0.2,
            AlertLogicalOp::And,
        )],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        ObserverApplicability::NotApplicable,
    )
}

pub(crate) fn get_mempool_transaction_drop_ratio_vec() -> Vec<Alert> {
    vec![get_mempool_transaction_drop_ratio(AlertSeverity::DayOnly)]
}

pub(crate) fn get_http_server_internal_error_once() -> Alert {
    Alert::new(
        "http_server_internal_error_once",
        "http server internal error once",
        AlertGroup::HttpServer,
        format!(
            "increase({}[20m]) or vector(0)",
            ADDED_TRANSACTIONS_INTERNAL_ERROR.get_name_with_filter()
        ),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 0.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::WorkingHours,
        ObserverApplicability::NotApplicable,
    )
}
