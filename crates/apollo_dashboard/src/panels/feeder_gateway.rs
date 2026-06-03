use apollo_feeder_gateway::metrics::FEEDER_GATEWAY_REQUESTS_TOTAL;
use apollo_metrics::metrics::MetricQueryName;

use crate::dashboard::Row;
use crate::panel::{Panel, PanelType};

fn get_panel_feeder_gateway_requests_rate() -> Panel {
    Panel::new(
        "Feeder Gateway Requests Rate (RPS)",
        "The rate of requests received by the feeder gateway (1m window)",
        format!("rate({}[1m])", FEEDER_GATEWAY_REQUESTS_TOTAL.get_name_with_filter()),
        PanelType::TimeSeries,
    )
}

pub(crate) fn get_feeder_gateway_row() -> Row {
    Row::new("Feeder Gateway", vec![get_panel_feeder_gateway_requests_rate()])
}
