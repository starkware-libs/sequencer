use apollo_http_server::metrics::{
    ADDED_TRANSACTIONS_FAILURE,
    ADDED_TRANSACTIONS_INTERNAL_ERROR,
    ADDED_TRANSACTIONS_SUCCESS,
    ADDED_TRANSACTIONS_TOTAL,
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

pub(crate) fn get_http_server_row() -> Row {
    Row::new(
        "Http Server",
        vec![
            get_panel_added_transactions_total(),
            get_panel_added_transactions_success(),
            get_panel_added_transactions_failure(),
            get_panel_added_transactions_internal_error(),
        ],
    )
}
