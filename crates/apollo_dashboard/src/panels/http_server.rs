use apollo_http_server::metrics::ADDED_TRANSACTIONS_TOTAL;

use crate::dashboard::{Panel, PanelType};

pub(crate) const PANEL_ADDED_TRANSACTIONS_TOTAL: Panel =
    Panel::from_counter(ADDED_TRANSACTIONS_TOTAL, PanelType::Stat);
