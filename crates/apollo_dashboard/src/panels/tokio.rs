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

use crate::dashboard::{Panel, Row};

fn get_panel_tokio_total_busy_duration_micros() -> Panel {
    Panel::from(&TOKIO_TOTAL_BUSY_DURATION_MICROS)
}
fn get_panel_tokio_min_busy_duration_micros() -> Panel {
    Panel::from(&TOKIO_MIN_BUSY_DURATION_MICROS)
}
fn get_panel_tokio_max_busy_duration_micros() -> Panel {
    Panel::from(&TOKIO_MAX_BUSY_DURATION_MICROS)
}

fn get_panel_tokio_total_park_count() -> Panel {
    Panel::from(&TOKIO_TOTAL_PARK_COUNT)
}
fn get_panel_tokio_min_park_count() -> Panel {
    Panel::from(&TOKIO_MIN_PARK_COUNT)
}
fn get_panel_tokio_max_park_count() -> Panel {
    Panel::from(&TOKIO_MAX_PARK_COUNT)
}
fn get_panel_tokio_global_queue_depth() -> Panel {
    Panel::from(&TOKIO_GLOBAL_QUEUE_DEPTH)
}
fn get_panel_tokio_workers_count() -> Panel {
    Panel::from(&TOKIO_WORKERS_COUNT)
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
