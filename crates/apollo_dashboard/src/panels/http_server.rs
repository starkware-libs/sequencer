use apollo_http_server::metrics::ADDED_TRANSACTIONS_TOTAL;

use crate::dashboard::{Panel, PanelType, Row};

const PANEL_ADDED_TRANSACTIONS_TOTAL: Panel =
    Panel::from_counter(ADDED_TRANSACTIONS_TOTAL, PanelType::TimeSeries);

pub(crate) fn get_http_server_row() -> Row {
    Row::new("Http Server", vec![PANEL_ADDED_TRANSACTIONS_TOTAL])
}
