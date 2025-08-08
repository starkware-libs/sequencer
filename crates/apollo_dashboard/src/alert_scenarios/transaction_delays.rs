use apollo_http_server::metrics::HTTP_SERVER_ADD_TX_LATENCY;
use apollo_mempool_p2p::metrics::MEMPOOL_P2P_NUM_CONNECTED_PEERS;

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

// TODO(shahak): add gateway latency alert

fn get_mempool_p2p_peer_down(
    alert_env_filtering: AlertEnvFiltering,
    alert_severity: AlertSeverity,
) -> Alert {
    Alert::new(
        "mempool_p2p_peer_down",
        "Mempool p2p peer down",
        AlertGroup::Mempool,
        format!("max_over_time({}[2m])", MEMPOOL_P2P_NUM_CONNECTED_PEERS.get_name_with_filter()),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            // TODO(shahak): find a way to make this depend on num_validators
            comparison_value: 2.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        alert_env_filtering,
    )
}

pub(crate) fn get_mempool_p2p_peer_down_vec() -> Vec<Alert> {
    vec![
        get_mempool_p2p_peer_down(AlertEnvFiltering::MainnetStyleAlerts, AlertSeverity::Regular),
        get_mempool_p2p_peer_down(
            AlertEnvFiltering::TestnetStyleAlerts,
            AlertSeverity::WorkingHours,
        ),
    ]
}

/// Triggers if the average latency of `add_tx` calls, across all HTTP servers, exceeds 2 seconds
/// over a 2-minute window.
fn get_http_server_avg_add_tx_latency_alert(
    alert_env_filtering: AlertEnvFiltering,
    alert_severity: AlertSeverity,
) -> Alert {
    let sum_metric = HTTP_SERVER_ADD_TX_LATENCY.get_name_sum_with_filter();
    let count_metric = HTTP_SERVER_ADD_TX_LATENCY.get_name_count_with_filter();

    Alert::new(
        "http_server_avg_add_tx_latency",
        "High HTTP server average add_tx latency",
        AlertGroup::HttpServer,
        format!("rate({sum_metric}[2m]) / rate({count_metric}[2m])"),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 2.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        alert_env_filtering,
    )
}

pub(crate) fn get_http_server_avg_add_tx_latency_alert_vec() -> Vec<Alert> {
    vec![
        get_http_server_avg_add_tx_latency_alert(
            AlertEnvFiltering::MainnetStyleAlerts,
            AlertSeverity::Regular,
        ),
        get_http_server_avg_add_tx_latency_alert(
            AlertEnvFiltering::TestnetStyleAlerts,
            AlertSeverity::WorkingHours,
        ),
    ]
}

/// Triggers when the slowest 5% of transactions for a specific HTTP server are taking longer than 2
/// seconds over a 5-minute window.
fn get_http_server_p95_add_tx_latency_alert(
    alert_env_filtering: AlertEnvFiltering,
    alert_severity: AlertSeverity,
) -> Alert {
    Alert::new(
        "http_server_p95_add_tx_latency",
        "High HTTP server P95 add_tx latency",
        AlertGroup::HttpServer,
        format!(
            "histogram_quantile(0.95, sum(rate({}[5m])) by (le))",
            HTTP_SERVER_ADD_TX_LATENCY.get_name_with_filter()
        ),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 2.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        alert_env_filtering,
    )
}

pub(crate) fn get_http_server_p95_add_tx_latency_alert_vec() -> Vec<Alert> {
    vec![
        get_http_server_p95_add_tx_latency_alert(
            AlertEnvFiltering::MainnetStyleAlerts,
            AlertSeverity::Regular,
        ),
        get_http_server_p95_add_tx_latency_alert(
            AlertEnvFiltering::TestnetStyleAlerts,
            AlertSeverity::WorkingHours,
        ),
    ]
}
