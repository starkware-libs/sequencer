use apollo_committer::metrics::{
    COMPUTE_DURATION_PER_BLOCK,
    COMPUTE_DURATION_PER_BLOCK_HIST,
    READ_DURATION_PER_BLOCK,
    READ_DURATION_PER_BLOCK_HIST,
    WRITE_DURATION_PER_BLOCK,
    WRITE_DURATION_PER_BLOCK_HIST,
};

use crate::dashboard::{Panel, PanelType, Row, Unit};

fn get_panel_read_duration_per_block() -> Panel {
    Panel::from_gauge(&READ_DURATION_PER_BLOCK, PanelType::TimeSeries).with_unit(Unit::Milliseconds)
}

fn get_panel_compute_duration_per_block() -> Panel {
    Panel::from_gauge(&COMPUTE_DURATION_PER_BLOCK, PanelType::TimeSeries)
        .with_unit(Unit::Milliseconds)
}

fn get_panel_write_duration_per_block() -> Panel {
    Panel::from_gauge(&WRITE_DURATION_PER_BLOCK, PanelType::TimeSeries)
        .with_unit(Unit::Milliseconds)
}

pub(crate) fn get_panel_read_duration_per_block_hist() -> Panel {
    Panel::from_hist(
        &READ_DURATION_PER_BLOCK_HIST,
        "Read Duration per Block Histogram",
        "Duration of the read operation per block",
    )
    .with_unit(Unit::Milliseconds)
}

pub(crate) fn get_panel_compute_duration_per_block_hist() -> Panel {
    Panel::from_hist(
        &COMPUTE_DURATION_PER_BLOCK_HIST,
        "Compute Duration per Block Histogram",
        "Duration of the compute operation per block",
    )
    .with_unit(Unit::Milliseconds)
}

pub(crate) fn get_panel_write_duration_per_block_hist() -> Panel {
    Panel::from_hist(
        &WRITE_DURATION_PER_BLOCK_HIST,
        "Write Duration per Block Histogram",
        "Duration of the write operation per block",
    )
    .with_unit(Unit::Milliseconds)
}

pub(crate) fn get_committer_row() -> Row {
    Row::new(
        "Committer",
        vec![
            get_panel_read_duration_per_block(),
            get_panel_read_duration_per_block_hist(),
            get_panel_compute_duration_per_block(),
            get_panel_compute_duration_per_block_hist(),
            get_panel_write_duration_per_block(),
            get_panel_write_duration_per_block_hist(),
        ],
    )
}
