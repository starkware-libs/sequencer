use apollo_batcher::metrics::NUM_TRANSACTION_IN_BLOCK;
use apollo_http_server::metrics::HTTP_SERVER_ADD_TX_LATENCY;
use apollo_infra::metrics::HISTOGRAM_BUCKETS;
use apollo_infra_utils::template::Template;
use apollo_mempool_p2p::metrics::MEMPOOL_P2P_NUM_CONNECTED_PEERS;
use apollo_metrics::metrics::MetricQueryName;

use crate::alert_placeholders::{
    format_sampling_window,
    ComparisonValueOrPlaceholder,
    ExpressionOrExpressionWithPlaceholder,
    SeverityValueOrPlaceholder,
};
use crate::alerts::{
    Alert,
    AlertComparisonOp,
    AlertCondition,
    AlertLogicalOp,
    EvaluationRate,
    ObserverApplicability,
    EVALUATION_INTERVAL_SEC_DEFAULT,
    PENDING_DURATION_DEFAULT,
};

// TODO(shahak): add gateway latency alert

pub(crate) fn get_mempool_p2p_peer_down() -> Alert {
    const ALERT_NAME: &str = "mempool_p2p_peer_down";
    Alert::new(
        ALERT_NAME,
        "Mempool p2p peer down",
        EvaluationRate::Default,
        format!("max_over_time({}[2m])", MEMPOOL_P2P_NUM_CONNECTED_PEERS.get_name_with_filter()),
        vec![AlertCondition::new(
            AlertComparisonOp::LessThan,
            // TODO(shahak): find a way to make this depend on num_validators
            2.0,
            AlertLogicalOp::And,
        )],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        SeverityValueOrPlaceholder::Placeholder(ALERT_NAME.to_string()),
        ObserverApplicability::NotApplicable,
    )
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
        EVALUATION_INTERVAL_SEC_DEFAULT,
        SeverityValueOrPlaceholder::Placeholder(ALERT_NAME.to_string()),
        ObserverApplicability::NotApplicable,
    )
}

/// Triggers if the latency of all `add_tx` calls, across all HTTP servers, exceeds 1 second
/// over a 2-minute window.
pub(crate) fn get_http_server_min_add_tx_latency_alert() -> Alert {
    const ALERT_NAME: &str = "http_server_min_add_tx_latency";
    const TIME_WINDOW: &str = "2m";
    let bucket_metric =
        HTTP_SERVER_ADD_TX_LATENCY.get_name_with_filer_and_additional_fields("le=\"1.0\"");
    let count_metric = HTTP_SERVER_ADD_TX_LATENCY.get_name_count_with_filter();
    Alert::new(
        ALERT_NAME,
        "High HTTP server minimal add_tx latency",
        EvaluationRate::Default,
        // The lhs expr checks that there were transaction observations during the time window.
        // The rhs expr verifies that none of these observations had a latency of 1 second or less
        // (i.e., the le="1.0" bucket is empty).
        // Multiplying these two conditions serves as a logical "and": it triggers only when there
        // was activity, and all observed transactions took longer than 1 second.
        format!(
            "(sum(increase({count_metric}[{TIME_WINDOW}])) > 0) * \
             (sum(increase({bucket_metric}[{TIME_WINDOW}])) < 1)"
        ),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 0.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
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
        EVALUATION_INTERVAL_SEC_DEFAULT,
        SeverityValueOrPlaceholder::ConcreteValue(crate::alerts::AlertSeverity::Informational),
        ObserverApplicability::NotApplicable,
    )
}

pub(crate) fn get_high_empty_blocks_ratio_alert() -> Alert {
    const ALERT_NAME: &str = "high_empty_blocks_ratio";
    // Our histogram buckets are static and the smallest bucket is 0.001.
    let lowest_histogram_bucket_value = HISTOGRAM_BUCKETS[0];
    let zero_bucket = NUM_TRANSACTION_IN_BLOCK.get_name_with_filer_and_additional_fields(&format!(
        "le=\"{lowest_histogram_bucket_value}\""
    ));
    let total_count = NUM_TRANSACTION_IN_BLOCK.get_name_count_with_filter();

    let expr_template_string = format!(
        "sum(increase({zero_bucket}[{{}}s])) / clamp_min(sum(increase({total_count}[{{}}s])), 1)"
    );

    Alert::new(
        ALERT_NAME,
        "High ratio of empty blocks",
        EvaluationRate::Default,
        ExpressionOrExpressionWithPlaceholder::Placeholder(
            Template::new(expr_template_string),
            vec![
                format_sampling_window(&format!("{}-zero_bucket", ALERT_NAME)),
                format_sampling_window(&format!("{}-total_count", ALERT_NAME)),
            ],
        ),
        vec![AlertCondition::new(
            AlertComparisonOp::GreaterThan,
            ComparisonValueOrPlaceholder::Placeholder(ALERT_NAME.to_string()),
            AlertLogicalOp::And,
        )],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        SeverityValueOrPlaceholder::Placeholder(ALERT_NAME.to_string()),
        ObserverApplicability::NotApplicable,
    )
}
