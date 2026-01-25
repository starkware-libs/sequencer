use apollo_committer::metrics::{
    COMPUTE_DURATION_PER_BLOCK,
    READ_DURATION_PER_BLOCK,
    WRITE_DURATION_PER_BLOCK,
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
