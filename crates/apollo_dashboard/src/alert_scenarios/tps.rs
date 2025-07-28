use apollo_gateway::metrics::GATEWAY_TRANSACTIONS_RECEIVED;
use apollo_http_server::metrics::ADDED_TRANSACTIONS_SUCCESS;
use apollo_mempool::metrics::MEMPOOL_TRANSACTIONS_RECEIVED;

use crate::alerts::{
    Alert,
    AlertComparisonOp,
    AlertCondition,
    AlertEnvFiltering,
    AlertGroup,
    AlertLogicalOp,
    AlertSeverity,
};

const PENDING_DURATION_DEFAULT: &str = "30s";
const EVALUATION_INTERVAL_SEC_DEFAULT: u64 = 30;

fn build_idle_alert(
    alert_name: &str,
    alert_title: &str,
    alert_group: AlertGroup,
    metric_name_with_filter: &str,
) -> Alert {
    Alert::new(
        alert_name,
        alert_title,
        alert_group,
        format!("sum(increase({}[2m])) or vector(0)", metric_name_with_filter),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 0.1,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Sos,
        AlertEnvFiltering::All,
    )
}

pub(crate) fn get_http_server_no_successful_transactions() -> Alert {
    build_idle_alert(
        "http_server_no_successful_transactions",
        "http server no successful transactions",
        AlertGroup::HttpServer,
        ADDED_TRANSACTIONS_SUCCESS.get_name_with_filter(),
    )
}

pub(crate) fn get_gateway_add_tx_idle() -> Alert {
    build_idle_alert(
        "gateway_add_tx_idle_all_sources",
        "Gateway add_tx idle (all sources)",
        AlertGroup::Gateway,
        GATEWAY_TRANSACTIONS_RECEIVED.get_name_with_filter(),
    )
}

pub(crate) fn get_mempool_add_tx_idle() -> Alert {
    build_idle_alert(
        "mempool_add_tx_idle_all_sources",
        "Mempool add_tx idle (all sources)",
        AlertGroup::Mempool,
        MEMPOOL_TRANSACTIONS_RECEIVED.get_name_with_filter(),
    )
}

fn get_http_server_low_successful_transaction_rate(
    alert_env_filtering: AlertEnvFiltering,
    alert_severity: AlertSeverity,
) -> Alert {
    Alert::new(
        "http_server_low_successful_transaction_rate",
        "http server low successful transaction rate",
        AlertGroup::HttpServer,
        format!(
            "increase({}[10m]) or vector(0)",
            ADDED_TRANSACTIONS_SUCCESS.get_name_with_filter()
        ),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 5.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        alert_env_filtering,
    )
}

pub(crate) fn get_http_server_low_successful_transaction_rate_vec() -> Vec<Alert> {
    vec![
        get_http_server_low_successful_transaction_rate(
            AlertEnvFiltering::MainnetStyleAlerts,
            AlertSeverity::DayOnly,
        ),
        get_http_server_low_successful_transaction_rate(
            AlertEnvFiltering::TestnetStyleAlerts,
            AlertSeverity::WorkingHours,
        ),
    ]
}
