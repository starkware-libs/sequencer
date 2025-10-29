use apollo_http_server::metrics::{
    ADDED_TRANSACTIONS_DEPRECATED_ERROR,
    ADDED_TRANSACTIONS_FAILURE,
    ADDED_TRANSACTIONS_INTERNAL_ERROR,
    ADDED_TRANSACTIONS_SUCCESS,
    ADDED_TRANSACTIONS_TOTAL,
    HTTP_SERVER_ADD_TX_LATENCY,
};

use crate::dashboard::{Panel, PanelType, Row, Unit};

fn get_panel_total_transactions_received() -> Panel {
    Panel::new(
        "Transactions Received",
        "Number of transactions received (10m window)",
        format!("increase({}[10m])", ADDED_TRANSACTIONS_TOTAL.get_name_with_filter()),
        PanelType::TimeSeries,
    )
    .with_log_query("\"ADD_TX_START\"")
}
fn get_panel_transaction_success_rate() -> Panel {
    Panel::new(
        "Transaction Success Rate",
        "The ratio of transactions successfully added to the gateway (10m window)",
        format!(
            "increase({}[10m]) / (increase({}[10m]) + increase({}[10m]))",
            ADDED_TRANSACTIONS_SUCCESS.get_name_with_filter(),
            ADDED_TRANSACTIONS_SUCCESS.get_name_with_filter(),
            ADDED_TRANSACTIONS_FAILURE.get_name_with_filter(),
        ),
        PanelType::TimeSeries,
    )
    .with_unit(Unit::PercentUnit)
    .with_log_query("\"Recorded transaction\"")
}
pub(crate) fn get_panel_http_server_transactions_received_rate() -> Panel {
    Panel::new(
        "HTTP Server Transactions Received Rate (TPS)",
        "The rate of transactions received by the HTTP Server (1m window)",
        format!("rate({}[1m])", ADDED_TRANSACTIONS_TOTAL.get_name_with_filter()),
        PanelType::TimeSeries,
    )
}
fn get_panel_http_add_tx_latency() -> Panel {
    Panel::from_hist(
        &HTTP_SERVER_ADD_TX_LATENCY,
        "HTTP Server Add Tx Latency",
        "The time it takes to add a transaction to the HTTP Server",
    )
    .with_unit(Unit::Seconds)
}

fn get_panel_transactions_failed_by_reason() -> Panel {
    Panel::new(
        "Transactions Failed to Be Added (By Reason)",
        "Number of transactions that failed to be added by reason (10m window)",
        vec![
            format!(
                "sum(increase({}[10m]))",
                ADDED_TRANSACTIONS_INTERNAL_ERROR.get_name_with_filter()
            ),
            format!(
                "sum(increase({}[10m]))",
                ADDED_TRANSACTIONS_DEPRECATED_ERROR.get_name_with_filter()
            ),
        ],
        PanelType::TimeSeries,
    )
    .with_legends(vec!["internal error", "deprecated error"])
}

pub(crate) fn get_http_server_row() -> Row {
    Row::new(
        "Http Server",
        vec![
            get_panel_http_server_transactions_received_rate(),
            get_panel_total_transactions_received(),
            get_panel_transaction_success_rate(),
            get_panel_transactions_failed_by_reason(),
            get_panel_http_add_tx_latency(),
        ],
    )
}
