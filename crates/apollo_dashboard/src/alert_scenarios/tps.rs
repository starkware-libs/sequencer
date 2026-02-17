use apollo_gateway::metrics::{
    GATEWAY_TRANSACTIONS_RECEIVED,
    GATEWAY_TRANSACTIONS_SENT_TO_MEMPOOL,
};
use apollo_http_server::metrics::ADDED_TRANSACTIONS_SUCCESS;
use apollo_infra_utils::template::Template;
use apollo_mempool::metrics::MEMPOOL_TRANSACTIONS_RECEIVED;
use apollo_metrics::metrics::MetricQueryName;

use crate::alert_placeholders::{format_sampling_window, ExpressionOrExpressionWithPlaceholder};
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

fn build_idle_alert(
    alert_name: &str,
    alert_title: &str,
    alert_group: AlertGroup,
    metric_name_with_filter: &str,
    alert_severity: AlertSeverity,
) -> Alert {
    let expr_template_string =
        format!("sum(increase({}[{{}}s])) or vector(0)", metric_name_with_filter);
    Alert::new(
        alert_name,
        alert_title,
        alert_group,
        ExpressionOrExpressionWithPlaceholder::Placeholder(
            Template::new(expr_template_string),
            vec![format_sampling_window(alert_name)],
        ),
        vec![AlertCondition::new(AlertComparisonOp::LessThan, 0.1, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        ObserverApplicability::NotApplicable,
    )
}

pub(crate) fn get_http_server_no_successful_transactions() -> Alert {
    build_idle_alert(
        "http_server_no_successful_transactions",
        "http server no successful transactions",
        AlertGroup::HttpServer,
        &ADDED_TRANSACTIONS_SUCCESS.get_name_with_filter(),
        AlertSeverity::Informational,
    )
}

pub(crate) fn get_gateway_add_tx_idle() -> Alert {
    build_idle_alert(
        "gateway_add_tx_idle_p2p_rpc",
        "Gateway add_tx idle (p2p+rpc)",
        AlertGroup::Gateway,
        &GATEWAY_TRANSACTIONS_RECEIVED.get_name_with_filter(),
        AlertSeverity::Regular,
    )
}

pub(crate) fn get_mempool_add_tx_idle() -> Alert {
    build_idle_alert(
        "mempool_add_tx_idle_p2p_rpc",
        "Mempool add_tx idle (p2p+rpc)",
        AlertGroup::Mempool,
        &MEMPOOL_TRANSACTIONS_RECEIVED.get_name_with_filter(),
        AlertSeverity::Sos,
    )
}

fn get_gateway_low_successful_transaction_rate(alert_severity: AlertSeverity) -> Alert {
    Alert::new(
        "gateway_low_successful_transaction_rate",
        "gateway low successful transaction rate",
        AlertGroup::Gateway,
        format!(
            "sum(increase({}[10m])) or vector(0)",
            GATEWAY_TRANSACTIONS_SENT_TO_MEMPOOL.get_name_with_filter()
        ),
        vec![AlertCondition::new(AlertComparisonOp::LessThan, 5.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        ObserverApplicability::NotApplicable,
    )
}

pub(crate) fn get_gateway_low_successful_transaction_rate_vec() -> Vec<Alert> {
    vec![get_gateway_low_successful_transaction_rate(AlertSeverity::DayOnly)]
}
