use apollo_http_server::metrics::HTTP_SERVER_ADD_TX_LATENCY;
use apollo_mempool_p2p::metrics::MEMPOOL_P2P_NUM_CONNECTED_PEERS;
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

#[cfg(test)]
#[path = "transaction_delays_test.rs"]
mod transaction_delays_test;

// TODO(shahak): add gateway latency alert

pub(crate) fn get_mempool_p2p_peer_down() -> Alert {
    const ALERT_NAME: &str = "mempool_p2p_peer_down";
    Alert::new(
        ALERT_NAME,
        "Mempool p2p peer down",
        EvaluationRate::Default,
        format!(
            "min(max_over_time({}[2m]))",
            MEMPOOL_P2P_NUM_CONNECTED_PEERS.get_name_with_filter()
        ),
        vec![AlertCondition::new(
            AlertComparisonOp::LessThan,
            // TODO(shahak): find a way to make this depend on num_validators
            2.0,
            AlertLogicalOp::And,
        )],
        PENDING_DURATION_DEFAULT,
        SeverityValueOrPlaceholder::Placeholder(ALERT_NAME.to_string()),
        ObserverApplicability::NotApplicable,
    )
    .with_no_data_fallback(2.0)
}

/// Triggers if the average latency of `add_tx` calls, across all HTTP servers, exceeds 15 seconds
/// over a 5-minute window.
pub(crate) fn get_http_server_avg_add_tx_latency_alert() -> Alert {
    const ALERT_NAME: &str = "http_server_avg_add_tx_latency";
    let sum_metric = HTTP_SERVER_ADD_TX_LATENCY.get_name_sum_with_filter();
    let count_metric = HTTP_SERVER_ADD_TX_LATENCY.get_name_count_with_filter();

    Alert::new(
        ALERT_NAME,
        "High HTTP server average add_tx latency",
        EvaluationRate::Default,
        // The clamp_min is used to avoid division by zero, and the minimal value
        // is 1/300, which is the minimum value of a valid count rate over a 5-minute window.
        format!("rate({sum_metric}[5m]) / clamp_min(rate({count_metric}[5m]), 1/300)"),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 15.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        SeverityValueOrPlaceholder::Placeholder(ALERT_NAME.to_string()),
        ObserverApplicability::NotApplicable,
    )
}

/// The `le` bound of the sub-second latency bucket, in seconds.
const MIN_LATENCY_BUCKET_SECONDS: f64 = 1.0;
/// Minimum `add_tx` calls in the window for the alert to evaluate.
const MIN_ADD_TX_CALLS_IN_WINDOW: f64 = 50.0;

/// Triggers if every `add_tx` call across all HTTP servers exceeded 1 second over a 2-minute
/// window, and the window carried at least [`MIN_ADD_TX_CALLS_IN_WINDOW`] calls.
pub(crate) fn get_http_server_min_add_tx_latency_alert() -> Alert {
    const ALERT_NAME: &str = "http_server_min_add_tx_latency";
    const TIME_WINDOW: &str = "2m";
    let bucket_metric = HTTP_SERVER_ADD_TX_LATENCY
        .get_name_with_filer_and_additional_fields(&format!("le=\"{MIN_LATENCY_BUCKET_SECONDS}\""));
    let count_metric = HTTP_SERVER_ADD_TX_LATENCY.get_name_count_with_filter();
    Alert::new(
        ALERT_NAME,
        "High HTTP server minimal add_tx latency",
        EvaluationRate::Default,
        // `bool` makes each comparison yield 1/0 rather than its own operand, so the product acts
        // as a logical "and": enough traffic in the window, and none of it sub-second.
        format!(
            "(sum(increase({count_metric}[{TIME_WINDOW}])) > bool {MIN_ADD_TX_CALLS_IN_WINDOW}) * \
             (sum(increase({bucket_metric}[{TIME_WINDOW}])) < bool 1)"
        ),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 0.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        SeverityValueOrPlaceholder::Placeholder(ALERT_NAME.to_string()),
        ObserverApplicability::NotApplicable,
    )
}

/// Triggers when the slowest 5% of transactions for a specific HTTP server are taking longer than 2
/// seconds over a 5-minute window.
pub(crate) fn get_http_server_p95_add_tx_latency_alert() -> Alert {
    Alert::new(
        "http_server_p95_add_tx_latency",
        "High HTTP server P95 add_tx latency",
        EvaluationRate::Default,
        format!(
            "histogram_quantile(0.95, sum(rate({}[5m])) by (le))",
            HTTP_SERVER_ADD_TX_LATENCY.get_name_with_filter()
        ),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 2.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        SeverityValueOrPlaceholder::ConcreteValue(crate::alerts::AlertSeverity::Informational),
        ObserverApplicability::NotApplicable,
    )
}
