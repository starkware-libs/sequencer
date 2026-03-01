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
use apollo_metrics::metrics::MetricDetails;

use crate::dashboard::Row;
use crate::infra_panels::POD_LEGEND;
use crate::panel::{Panel, PanelType, Unit};
use crate::query_builder::{sum_by_pod, DisplayMethod};

fn get_panel_tokio_total_busy_duration_micros() -> Panel {
    Panel::new(
        "Increase of Tokio total busy duration (1m window)",
        TOKIO_TOTAL_BUSY_DURATION_MICROS.get_description(),
        sum_by_pod(&TOKIO_TOTAL_BUSY_DURATION_MICROS, DisplayMethod::Increase("1m")),
        PanelType::TimeSeries,
    )
    .with_legends(POD_LEGEND)
    .with_unit(Unit::Microseconds)
}
fn get_panel_tokio_min_busy_duration_micros() -> Panel {
    Panel::new(
        "Tokio minimal busy duration",
        TOKIO_MIN_BUSY_DURATION_MICROS.get_description(),
        sum_by_pod(&TOKIO_MIN_BUSY_DURATION_MICROS, DisplayMethod::Raw),
        PanelType::TimeSeries,
    )
    .with_legends(POD_LEGEND)
    .with_unit(Unit::Microseconds)
}
fn get_panel_tokio_max_busy_duration_micros() -> Panel {
    Panel::new(
        "Tokio maximal busy duration",
        TOKIO_MAX_BUSY_DURATION_MICROS.get_description(),
        sum_by_pod(&TOKIO_MAX_BUSY_DURATION_MICROS, DisplayMethod::Raw),
        PanelType::TimeSeries,
    )
    .with_legends(POD_LEGEND)
    .with_unit(Unit::Microseconds)
}

fn get_panel_tokio_total_park_count() -> Panel {
    Panel::new(
        "Tokio Total Park Count",
        TOKIO_TOTAL_PARK_COUNT.get_description(),
        sum_by_pod(&TOKIO_TOTAL_PARK_COUNT, DisplayMethod::Raw),
        PanelType::TimeSeries,
    )
    .with_legends(POD_LEGEND)
}
fn get_panel_tokio_min_park_count() -> Panel {
    Panel::new(
        "Tokio Min Park Count",
        TOKIO_MIN_PARK_COUNT.get_description(),
        sum_by_pod(&TOKIO_MIN_PARK_COUNT, DisplayMethod::Raw),
        PanelType::TimeSeries,
    )
    .with_legends(POD_LEGEND)
}
fn get_panel_tokio_max_park_count() -> Panel {
    Panel::new(
        "Tokio Max Park Count",
        TOKIO_MAX_PARK_COUNT.get_description(),
        sum_by_pod(&TOKIO_MAX_PARK_COUNT, DisplayMethod::Raw),
        PanelType::TimeSeries,
    )
    .with_legends(POD_LEGEND)
}
fn get_panel_tokio_global_queue_depth() -> Panel {
    Panel::new(
        "Tokio Global Queue Depth",
        TOKIO_GLOBAL_QUEUE_DEPTH.get_description(),
        sum_by_pod(&TOKIO_GLOBAL_QUEUE_DEPTH, DisplayMethod::Raw),
        PanelType::TimeSeries,
    )
    .with_legends(POD_LEGEND)
}
fn get_panel_tokio_workers_count() -> Panel {
    Panel::new(
        "Tokio Workers Count",
        TOKIO_WORKERS_COUNT.get_description(),
        sum_by_pod(&TOKIO_WORKERS_COUNT, DisplayMethod::Raw),
        PanelType::TimeSeries,
    )
    .with_legends(POD_LEGEND)
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
