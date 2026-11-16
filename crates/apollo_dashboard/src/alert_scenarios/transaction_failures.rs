use apollo_http_server::metrics::{
    ADDED_TRANSACTIONS_DEPRECATED_ERROR,
    ADDED_TRANSACTIONS_INTERNAL_ERROR,
    ADDED_TRANSACTIONS_TOTAL,
};
use apollo_mempool::metrics::{DropReason, LABEL_NAME_DROP_REASON, MEMPOOL_TRANSACTIONS_DROPPED};
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
        SeverityValueOrPlaceholder::Placeholder(ALERT_NAME.to_string()),
        ObserverApplicability::NotApplicable,
    )
}

/// Expirations behind a nonce gap are the client's doing (see `DropReason::ExpiredBehindGap`), so
/// they are excluded from the count.
pub(crate) fn get_mempool_transaction_drop_ratio() -> Alert {
    const ALERT_NAME: &str = "mempool_transaction_drop_ratio";
    const MAX_DROPPED_PER_HOUR: f64 = 500.0;
    let behind_gap: &str = DropReason::ExpiredBehindGap.into();
    let dropped = MEMPOOL_TRANSACTIONS_DROPPED.get_name_with_filer_and_additional_fields(&format!(
        "{LABEL_NAME_DROP_REASON}!=\"{behind_gap}\""
    ));
    Alert::new(
        ALERT_NAME,
        "Mempool transactions dropped",
        EvaluationRate::Default,
        format!("sum(increase({dropped}[1h]))"),
        vec![AlertCondition::new(
            AlertComparisonOp::GreaterThan,
            // TODO(matanl): Decide on the final threshold and who should be alerted.
            MAX_DROPPED_PER_HOUR,
            AlertLogicalOp::And,
        )],
        PENDING_DURATION_DEFAULT,
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
        AlertSeverity::WorkingHours,
        ObserverApplicability::NotApplicable,
    )
}
