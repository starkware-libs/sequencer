use apollo_http_server::metrics::{
    ADDED_TRANSACTIONS_DEPRECATED_ERROR,
    ADDED_TRANSACTIONS_FAILURE,
    ADDED_TRANSACTIONS_INTERNAL_ERROR,
    ADDED_TRANSACTIONS_TOTAL,
};
use apollo_mempool::metrics::{MEMPOOL_TRANSACTIONS_DROPPED, MEMPOOL_TRANSACTIONS_RECEIVED};
use apollo_metrics::metrics::MetricQueryName;

use crate::alert_placeholders::SeverityValueOrPlaceholder;
use crate::alerts::{
    Alert,
    AlertComparisonOp,
    AlertCondition,
    AlertLogicalOp,
    AlertSeverity,
    EvaluationRate,
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
        EvaluationRate::Default,
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
        EvaluationRate::Default,
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

pub(crate) fn get_http_server_internal_error_ratio() -> Alert {
    const ALERT_NAME: &str = "http_server_internal_error_ratio";
    Alert::new(
        ALERT_NAME,
        "http server internal error ratio",
        EvaluationRate::Default,
        format!(
            "increase({}[1h]) / clamp_min(increase({}[1h]), 1)",
            ADDED_TRANSACTIONS_INTERNAL_ERROR.get_name_with_filter(),
            ADDED_TRANSACTIONS_TOTAL.get_name_with_filter()
        ),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 0.01, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        SeverityValueOrPlaceholder::Placeholder(ALERT_NAME.to_string()),
        ObserverApplicability::NotApplicable,
    )
}

pub(crate) fn get_mempool_transaction_drop_ratio() -> Alert {
    const ALERT_NAME: &str = "mempool_transaction_drop_ratio";
    Alert::new(
        ALERT_NAME,
        "Mempool transaction drop ratio",
        EvaluationRate::Default,
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
        SeverityValueOrPlaceholder::Placeholder(ALERT_NAME.to_string()),
        ObserverApplicability::NotApplicable,
    )
}

pub(crate) fn get_http_server_internal_error_once() -> Alert {
    Alert::new(
        "http_server_internal_error_once",
        "http server internal error once",
        EvaluationRate::Default,
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
