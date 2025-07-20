use apollo_http_server::metrics::{
    ADDED_TRANSACTIONS_DEPRECATED_ERROR,
    ADDED_TRANSACTIONS_FAILURE,
    ADDED_TRANSACTIONS_INTERNAL_ERROR,
    ADDED_TRANSACTIONS_SUCCESS,
    ADDED_TRANSACTIONS_TOTAL,
    HTTP_SERVER_ADD_TX_LATENCY,
};

use crate::dashboard::{Panel, PanelType, Row};

fn get_panel_added_transactions_total() -> Panel {
    Panel::from_counter(ADDED_TRANSACTIONS_TOTAL, PanelType::TimeSeries)
}

fn get_panel_added_transactions_success() -> Panel {
    Panel::from_counter(ADDED_TRANSACTIONS_SUCCESS, PanelType::TimeSeries)
}

fn get_panel_added_transactions_failure() -> Panel {
    Panel::from_counter(ADDED_TRANSACTIONS_FAILURE, PanelType::TimeSeries)
}

fn get_panel_added_transactions_internal_error() -> Panel {
    Panel::from_counter(ADDED_TRANSACTIONS_INTERNAL_ERROR, PanelType::TimeSeries)
}

fn get_panel_added_transactions_deprecated_error() -> Panel {
    Panel::from_counter(ADDED_TRANSACTIONS_DEPRECATED_ERROR, PanelType::TimeSeries)
}

fn get_panel_http_server_transactions_received_rate() -> Panel {
    Panel::new(
        "http_server_transactions_received_rate (TPS)",
        "The rate of transactions received by the HTTP Server during the last 20 minutes",
        vec![format!(
            "sum(rate({}[20m])) or vector(0)",
            ADDED_TRANSACTIONS_TOTAL.get_name_with_filter()
        )],
        PanelType::TimeSeries,
    )
}

fn get_panel_http_add_tx_latency() -> Panel {
    Panel::from_hist(HTTP_SERVER_ADD_TX_LATENCY, PanelType::TimeSeries)
}

pub(crate) fn get_http_server_row() -> Row {
    Row::new(
        "Http Server",
        vec![
            get_panel_added_transactions_total(),
            get_panel_http_server_transactions_received_rate(),
            get_panel_added_transactions_success(),
            get_panel_added_transactions_failure(),
            get_panel_added_transactions_internal_error(),
            get_panel_added_transactions_deprecated_error(),
            get_panel_http_add_tx_latency(),
        ],
    )
}
