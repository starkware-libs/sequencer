use apollo_committer::metrics::{
    COMPUTE_DURATION_PER_BLOCK,
    COMPUTE_DURATION_PER_BLOCK_HIST,
    READ_DB_ENTRIES_PER_BLOCK,
    READ_DURATION_PER_BLOCK,
    READ_DURATION_PER_BLOCK_HIST,
    WRITE_DB_ENTRIES_PER_BLOCK,
    WRITE_DURATION_PER_BLOCK,
    WRITE_DURATION_PER_BLOCK_HIST,
};
use apollo_metrics::metrics::MetricQueryName;

use crate::dashboard::{Panel, PanelType, Row, Unit};

fn get_panel_read_duration_per_block() -> Panel {
    Panel::from_gauge_and_hist(&READ_DURATION_PER_BLOCK, &READ_DURATION_PER_BLOCK_HIST)
        .with_unit(Unit::Milliseconds)
}

fn get_panel_compute_duration_per_block() -> Panel {
    Panel::from_gauge_and_hist(&COMPUTE_DURATION_PER_BLOCK, &COMPUTE_DURATION_PER_BLOCK_HIST)
        .with_unit(Unit::Milliseconds)
}

fn get_panel_write_duration_per_block() -> Panel {
    Panel::from_gauge_and_hist(&WRITE_DURATION_PER_BLOCK, &WRITE_DURATION_PER_BLOCK_HIST)
        .with_unit(Unit::Milliseconds)
}

pub(crate) fn get_panel_read_duration_per_read_entry() -> Panel {
    Panel::new(
        "Average Read Duration per Read Entry",
        "Average duration of the read operation per read entry in a block",
        format!(
            "{} / ({} > 0)",
            READ_DURATION_PER_BLOCK.get_name_with_filter(),
            READ_DB_ENTRIES_PER_BLOCK.get_name_with_filter()
        ),
        PanelType::TimeSeries,
    )
    .with_unit(Unit::Milliseconds)
}

pub(crate) fn get_panel_compute_duration_per_write_entry() -> Panel {
    Panel::new(
        "Average Compute Duration per Write Entry",
        "Average duration of the compute operation per write entry in a block",
        format!(
            "{} / ({} > 0)",
            COMPUTE_DURATION_PER_BLOCK.get_name_with_filter(),
            WRITE_DB_ENTRIES_PER_BLOCK.get_name_with_filter()
        ),
        PanelType::TimeSeries,
    )
    .with_unit(Unit::Milliseconds)
}

pub(crate) fn get_panel_write_duration_per_write_entry() -> Panel {
    Panel::new(
        "Average Write Duration per Write Entry",
        "Average duration of the write operation per write entry in a block",
        format!(
            "{} / ({} > 0)",
            WRITE_DURATION_PER_BLOCK.get_name_with_filter(),
            WRITE_DB_ENTRIES_PER_BLOCK.get_name_with_filter()
        ),
        PanelType::TimeSeries,
    )
    .with_unit(Unit::Milliseconds)
}

pub(crate) fn get_committer_row() -> Row {
    Row::new(
        "Committer",
        vec![
            get_panel_read_duration_per_block(),
            get_panel_read_duration_per_read_entry(),
            get_panel_compute_duration_per_block(),
            get_panel_compute_duration_per_write_entry(),
            get_panel_write_duration_per_block(),
            get_panel_write_duration_per_write_entry(),
        ],
    )
}
