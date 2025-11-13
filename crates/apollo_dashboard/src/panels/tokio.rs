use apollo_metrics::MetricCommon;
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

use crate::dashboard::{Panel, PanelType, Row, Unit};
const TOKIO_PANEL_LEGENDS: &[&str] = &["{{pod}}"];

fn get_panel_tokio_total_busy_duration_micros() -> Panel {
    Panel::new(
        "Tokio total busy duration",
        TOKIO_TOTAL_BUSY_DURATION_MICROS.get_description(),
        TOKIO_TOTAL_BUSY_DURATION_MICROS.get_name_with_filter().to_string(),
        PanelType::TimeSeries,
    )
    .with_legends(TOKIO_PANEL_LEGENDS.to_vec())
    .with_unit(Unit::Microseconds)
}
fn get_panel_tokio_min_busy_duration_micros() -> Panel {
    Panel::new(
        "Tokio minimal busy duration",
        TOKIO_MIN_BUSY_DURATION_MICROS.get_description(),
        TOKIO_MIN_BUSY_DURATION_MICROS.get_name_with_filter().to_string(),
        PanelType::TimeSeries,
    )
    .with_legends(TOKIO_PANEL_LEGENDS.to_vec())
    .with_unit(Unit::Microseconds)
}
fn get_panel_tokio_max_busy_duration_micros() -> Panel {
    Panel::new(
        "Tokio maximal busy duration",
        TOKIO_MAX_BUSY_DURATION_MICROS.get_description(),
        TOKIO_MAX_BUSY_DURATION_MICROS.get_name_with_filter().to_string(),
        PanelType::TimeSeries,
    )
    .with_legends(TOKIO_PANEL_LEGENDS.to_vec())
    .with_unit(Unit::Microseconds)
}

fn get_panel_tokio_total_park_count() -> Panel {
    Panel::from_gauge(&TOKIO_TOTAL_PARK_COUNT, PanelType::TimeSeries)
        .with_legends(TOKIO_PANEL_LEGENDS.to_vec())
}
fn get_panel_tokio_min_park_count() -> Panel {
    Panel::from_gauge(&TOKIO_MIN_PARK_COUNT, PanelType::TimeSeries)
        .with_legends(TOKIO_PANEL_LEGENDS.to_vec())
}
fn get_panel_tokio_max_park_count() -> Panel {
    Panel::from_gauge(&TOKIO_MAX_PARK_COUNT, PanelType::TimeSeries)
        .with_legends(TOKIO_PANEL_LEGENDS.to_vec())
}
fn get_panel_tokio_global_queue_depth() -> Panel {
    Panel::from_gauge(&TOKIO_GLOBAL_QUEUE_DEPTH, PanelType::TimeSeries)
        .with_legends(TOKIO_PANEL_LEGENDS.to_vec())
}
fn get_panel_tokio_workers_count() -> Panel {
    Panel::from_gauge(&TOKIO_WORKERS_COUNT, PanelType::TimeSeries)
        .with_legends(TOKIO_PANEL_LEGENDS.to_vec())
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
