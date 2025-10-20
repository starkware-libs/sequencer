use apollo_monitoring_endpoint::tokio_metrics::{
    TOKIO_GLOBAL_QUEUE_DEPTH,
    TOKIO_MAX_BUSY_DURATION_MICROS,
    TOKIO_MAX_PARK_COUNT,
    TOKIO_MIN_BUSY_DURATION_MICROS,
    TOKIO_MIN_PARK_COUNT,
    TOKIO_TOTAL_BUSY_DURATION_MICROS,
    TOKIO_TOTAL_PARK_COUNT,
    TOKIO_WORKERS_COUNT,
};

use crate::dashboard::{Panel, PanelType, Row};

fn get_panel_tokio_total_busy_duration_micros() -> Panel {
    Panel::from_counter(&TOKIO_TOTAL_BUSY_DURATION_MICROS, PanelType::TimeSeries)
        .with_legends(vec!["{{pod}}"])
}
fn get_panel_tokio_min_busy_duration_micros() -> Panel {
    Panel::from_counter(&TOKIO_MIN_BUSY_DURATION_MICROS, PanelType::TimeSeries)
}
fn get_panel_tokio_max_busy_duration_micros() -> Panel {
    Panel::from_counter(&TOKIO_MAX_BUSY_DURATION_MICROS, PanelType::TimeSeries)
}

fn get_panel_tokio_total_park_count() -> Panel {
    Panel::from_gauge(&TOKIO_TOTAL_PARK_COUNT, PanelType::TimeSeries)
}
fn get_panel_tokio_min_park_count() -> Panel {
    Panel::from_gauge(&TOKIO_MIN_PARK_COUNT, PanelType::TimeSeries)
}
fn get_panel_tokio_max_park_count() -> Panel {
    Panel::from_gauge(&TOKIO_MAX_PARK_COUNT, PanelType::TimeSeries)
}
fn get_panel_tokio_global_queue_depth() -> Panel {
    Panel::from_gauge(&TOKIO_GLOBAL_QUEUE_DEPTH, PanelType::TimeSeries)
}
fn get_panel_tokio_workers_count() -> Panel {
    Panel::from_gauge(&TOKIO_WORKERS_COUNT, PanelType::TimeSeries)
}

pub(crate) fn get_tokio_row() -> Row {
    Row::new(
        "Tokio Runtime Metrics",
        vec![
            get_panel_tokio_total_busy_duration_micros(),
            get_panel_tokio_min_busy_duration_micros(),
            get_panel_tokio_max_busy_duration_micros(),
            get_panel_tokio_total_park_count(),
            get_panel_tokio_min_park_count(),
            get_panel_tokio_max_park_count(),
            get_panel_tokio_global_queue_depth(),
            get_panel_tokio_workers_count(),
        ],
    )
}
