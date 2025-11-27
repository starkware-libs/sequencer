use apollo_infra::tokio_metrics::{
    TOKIO_GLOBAL_QUEUE_DEPTH,
    TOKIO_MAX_BUSY_DURATION_MICROS,
    TOKIO_MAX_PARK_COUNT,
    TOKIO_MIN_BUSY_DURATION_MICROS,
    TOKIO_MIN_PARK_COUNT,
    TOKIO_TOTAL_BUSY_DURATION_MICROS,
    TOKIO_TOTAL_PARK_COUNT,
    TOKIO_WORKERS_COUNT,
};
use apollo_metrics::metrics::{MetricDetails, MetricQueryName};

use crate::dashboard::{Panel, PanelType, Row, Unit};
use crate::infra_panels::POD_LEGEND;
use crate::query_builder::increase;

fn get_panel_tokio_total_busy_duration_micros() -> Panel {
    Panel::new(
        "Increase of Tokio total busy duration (1m window)",
        TOKIO_TOTAL_BUSY_DURATION_MICROS.get_description(),
        increase(&TOKIO_TOTAL_BUSY_DURATION_MICROS, "1m"),
        PanelType::TimeSeries,
    )
    .with_legends(POD_LEGEND)
    .with_unit(Unit::Microseconds)
}
fn get_panel_tokio_min_busy_duration_micros() -> Panel {
    Panel::new(
        "Tokio minimal busy duration",
        TOKIO_MIN_BUSY_DURATION_MICROS.get_description(),
        TOKIO_MIN_BUSY_DURATION_MICROS.get_name_with_filter().to_string(),
        PanelType::TimeSeries,
    )
    .with_legends(POD_LEGEND)
    .with_unit(Unit::Microseconds)
}
fn get_panel_tokio_max_busy_duration_micros() -> Panel {
    Panel::new(
        "Tokio maximal busy duration",
        TOKIO_MAX_BUSY_DURATION_MICROS.get_description(),
        TOKIO_MAX_BUSY_DURATION_MICROS.get_name_with_filter().to_string(),
        PanelType::TimeSeries,
    )
    .with_legends(POD_LEGEND)
    .with_unit(Unit::Microseconds)
}

fn get_panel_tokio_total_park_count() -> Panel {
    Panel::from_gauge(&TOKIO_TOTAL_PARK_COUNT, PanelType::TimeSeries).with_legends(POD_LEGEND)
}
fn get_panel_tokio_min_park_count() -> Panel {
    Panel::from_gauge(&TOKIO_MIN_PARK_COUNT, PanelType::TimeSeries).with_legends(POD_LEGEND)
}
fn get_panel_tokio_max_park_count() -> Panel {
    Panel::from_gauge(&TOKIO_MAX_PARK_COUNT, PanelType::TimeSeries).with_legends(POD_LEGEND)
}
fn get_panel_tokio_global_queue_depth() -> Panel {
    Panel::from_gauge(&TOKIO_GLOBAL_QUEUE_DEPTH, PanelType::TimeSeries).with_legends(POD_LEGEND)
}
fn get_panel_tokio_workers_count() -> Panel {
    Panel::from_gauge(&TOKIO_WORKERS_COUNT, PanelType::TimeSeries).with_legends(POD_LEGEND)
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
