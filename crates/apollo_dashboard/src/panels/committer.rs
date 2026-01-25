use apollo_committer::metrics::{
    COMPUTE_DURATION_PER_BLOCK,
    COMPUTE_DURATION_PER_BLOCK_HIST,
    READ_DURATION_PER_BLOCK,
    READ_DURATION_PER_BLOCK_HIST,
    WRITE_DURATION_PER_BLOCK,
    WRITE_DURATION_PER_BLOCK_HIST,
};

use crate::dashboard::{Panel, Row, Unit};

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

pub(crate) fn get_committer_row() -> Row {
    Row::new(
        "Committer",
        vec![
            get_panel_read_duration_per_block(),
            get_panel_compute_duration_per_block(),
            get_panel_write_duration_per_block(),
        ],
    )
}
