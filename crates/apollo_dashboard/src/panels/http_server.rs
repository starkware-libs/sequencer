use apollo_http_server::metrics::{
    ADDED_TRANSACTIONS_DEPRECATED_ERROR,
    ADDED_TRANSACTIONS_INTERNAL_ERROR,
    ADDED_TRANSACTIONS_SUCCESS,
    ADDED_TRANSACTIONS_TOTAL,
    HTTP_SERVER_ADD_TX_LATENCY,
};

use crate::dashboard::{Panel, PanelType, Row, Unit, HISTOGRAM_QUANTILES, HISTOGRAM_TIME_RANGE};

fn get_panel_total_transactions_received() -> Panel {
    Panel::new(
        "Number of Total Transactions Received",
        "Number of total transactions received (10m window)",
        vec![format!("increase({}[10m])", ADDED_TRANSACTIONS_TOTAL.get_name_with_filter())],
        PanelType::Stat,
    )
    .with_log_query("\"ADD_TX_START\"")
}
fn get_panel_transactions_added_successfully() -> Panel {
    Panel::new(
        "Number of Transactions Successfully Added",
        "Number of transactions successfully added (10m window)",
        vec![format!("increase({}[10m])", ADDED_TRANSACTIONS_SUCCESS.get_name_with_filter())],
        PanelType::Stat,
    )
    .with_log_query("\"Recorded transaction\"")
}
fn get_panel_transactions_failed_to_be_added() -> Panel {
    Panel::new(
        "Number of Transactions Failed to be Added",
        "Number of transactions that failed to be added (10m window)",
        vec![format!(
            "increase({}[10m]) - increase({}[10m])",
            ADDED_TRANSACTIONS_TOTAL.get_name_with_filter(),
            ADDED_TRANSACTIONS_SUCCESS.get_name_with_filter()
        )],
        PanelType::Stat,
    )
}
fn get_panel_transactions_failed_to_be_added_due_to_internal_error() -> Panel {
    Panel::new(
        "Number of Transactions Failed to be Added Due to Internal Error",
        "Number of transactions that failed to be added due to an internal error (10m window)",
        vec![format!(
            "increase({}[10m])",
            ADDED_TRANSACTIONS_INTERNAL_ERROR.get_name_with_filter()
        )],
        PanelType::Stat,
    )
}
fn get_panel_transactions_failed_to_be_added_due_to_deprecated_error() -> Panel {
    Panel::new(
        "Number of Transactions Failed to be Added Due to Deprecated Error",
        "Number of transactions that failed to be added due to a deprecated error (10m window)",
        vec![format!(
            "increase({}[10m])",
            ADDED_TRANSACTIONS_DEPRECATED_ERROR.get_name_with_filter()
        )],
        PanelType::Stat,
    )
}
fn get_panel_http_server_transactions_received_rate() -> Panel {
    Panel::new(
        "Transactions Received Rate (TPS)",
        "The rate of transactions received by the HTTP Server (1m window)",
        vec![format!("rate({}[1m])", ADDED_TRANSACTIONS_TOTAL.get_name_with_filter())],
        PanelType::TimeSeries,
    )
}
fn get_panel_http_add_tx_latency() -> Panel {
    Panel::new(
        "HTTP Server Add Tx Latency",
        "The time it takes to add a transaction to the HTTP Server",
        HISTOGRAM_QUANTILES
            .iter()
            .map(|q| {
                format!(
                    "histogram_quantile({q:.2}, sum by (le) (rate({}[{HISTOGRAM_TIME_RANGE}])))",
                    HTTP_SERVER_ADD_TX_LATENCY.get_name_with_filter(),
                )
            })
            .collect(),
        PanelType::TimeSeries,
    )
    .with_unit(Unit::Seconds)
}

pub(crate) fn get_http_server_row() -> Row {
    Row::new(
        "Http Server",
        vec![
            get_panel_http_server_transactions_received_rate(),
            get_panel_total_transactions_received(),
            get_panel_transactions_added_successfully(),
            get_panel_transactions_failed_to_be_added(),
            get_panel_transactions_failed_to_be_added_due_to_internal_error(),
            get_panel_transactions_failed_to_be_added_due_to_deprecated_error(),
            get_panel_http_add_tx_latency(),
        ],
    )
}
