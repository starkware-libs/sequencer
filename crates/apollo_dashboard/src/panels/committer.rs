use apollo_committer::metrics::{
    COMPUTE_DURATION_PER_BLOCK,
    OFFSET,
    READ_DURATION_PER_BLOCK,
    WRITE_DURATION_PER_BLOCK,
};

use crate::dashboard::{Panel, PanelType, Row, Unit};

pub(crate) fn get_committer_row() -> Row {
    Row::new(
        "Committer",
        vec![
            Panel::from_gauge(&OFFSET, PanelType::TimeSeries),
            Panel::from_gauge_histogram(&READ_DURATION_PER_BLOCK).with_unit(Unit::Seconds),
            Panel::from_gauge_histogram(&COMPUTE_DURATION_PER_BLOCK).with_unit(Unit::Seconds),
            Panel::from_gauge_histogram(&WRITE_DURATION_PER_BLOCK).with_unit(Unit::Seconds),
        ],
    )
}
