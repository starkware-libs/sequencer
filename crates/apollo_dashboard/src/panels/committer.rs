use apollo_committer::metrics::{
    AVERAGE_COMPUTE_RATE,
    AVERAGE_READ_RATE,
    AVERAGE_WRITE_RATE,
    COMPUTE_DURATION_PER_BLOCK,
    OFFSET,
    READ_DURATION_PER_BLOCK,
    TOTAL_BLOCK_DURATION,
    WRITE_DURATION_PER_BLOCK,
};

use crate::dashboard::{Panel, PanelType, Row, Unit};

const BLOCK_DURATIONS_LOG_QUERY: &str = "Total/read/compute/write duration of block";

fn get_total_block_duration_panel() -> Panel {
    Panel::from_hist(&TOTAL_BLOCK_DURATION, "Total Block Duration", "Total block duration")
        .with_unit(Unit::Seconds)
        .with_log_query(BLOCK_DURATIONS_LOG_QUERY)
}

fn get_read_duration_per_block_panel() -> Panel {
    Panel::from_hist(&READ_DURATION_PER_BLOCK, "Read Duration per Block", "Read duration per block")
        .with_unit(Unit::Seconds)
        .with_log_query(BLOCK_DURATIONS_LOG_QUERY)
}

fn get_compute_duration_per_block_panel() -> Panel {
    Panel::from_hist(
        &COMPUTE_DURATION_PER_BLOCK,
        "Compute Duration per Block",
        "Compute duration per block",
    )
    .with_unit(Unit::Seconds)
    .with_log_query(BLOCK_DURATIONS_LOG_QUERY)
}

fn get_write_duration_per_block_panel() -> Panel {
    Panel::from_hist(
        &WRITE_DURATION_PER_BLOCK,
        "Write Duration per Block",
        "Write duration per block",
    )
    .with_unit(Unit::Seconds)
    .with_log_query(BLOCK_DURATIONS_LOG_QUERY)
}

fn get_average_read_rate_panel() -> Panel {
    Panel::from_hist(
        &AVERAGE_READ_RATE,
        "Average Read Rate (entries/sec)",
        "Average read rate over a block",
    )
    .with_log_query("Average read rate of block")
}

fn get_average_compute_rate_panel() -> Panel {
    Panel::from_hist(
        &AVERAGE_COMPUTE_RATE,
        "Average Compute Rate (entries/sec)",
        "Average compute rate over a block",
    )
    .with_log_query("Average compute rate of block")
}

fn get_average_write_rate_panel() -> Panel {
    Panel::from_hist(
        &AVERAGE_WRITE_RATE,
        "Average Write Rate (entries/sec)",
        "Average write rate over a block",
    )
    .with_log_query("Average write rate of block")
}

pub(crate) fn get_committer_row() -> Row {
    Row::new(
        "Committer",
        vec![
            Panel::from_gauge(&OFFSET, PanelType::TimeSeries),
            get_total_block_duration_panel(),
            get_read_duration_per_block_panel(),
            get_average_read_rate_panel(),
            get_compute_duration_per_block_panel(),
            get_average_compute_rate_panel(),
            get_write_duration_per_block_panel(),
            get_average_write_rate_panel(),
        ],
    )
}
