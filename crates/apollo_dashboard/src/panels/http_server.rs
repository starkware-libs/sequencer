use apollo_http_server::metrics::ADDED_TRANSACTIONS_TOTAL;

use crate::dashboard::{Panel, PanelType, Row};

fn get_panel_added_transactions_total() -> Panel {
    Panel::from_counter(ADDED_TRANSACTIONS_TOTAL, PanelType::Graph)
}

pub(crate) fn get_http_server_row() -> Row {
    Row::new("Http Server", vec![get_panel_added_transactions_total()])
}
