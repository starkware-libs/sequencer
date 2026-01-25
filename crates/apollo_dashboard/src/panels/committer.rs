use apollo_committer::metrics::{
    COMPUTE_DURATION_PER_BLOCK,
    COMPUTE_DURATION_PER_BLOCK_HIST,
    READ_DURATION_PER_BLOCK,
    READ_DURATION_PER_BLOCK_HIST,
    WRITE_DURATION_PER_BLOCK,
    WRITE_DURATION_PER_BLOCK_HIST,
};
use apollo_metrics::metrics::{MetricGauge, MetricHistogram};

use crate::dashboard::{Panel, Row, Unit};

fn ms_gauge_and_hist_panel(gauge: &MetricGauge, hist: &MetricHistogram) -> Panel {
    Panel::from_gauge_and_hist(gauge, hist).with_unit(Unit::Milliseconds)
}

pub(crate) fn get_committer_row() -> Row {
    Row::new(
        "Committer",
        vec![
            ms_gauge_and_hist_panel(&READ_DURATION_PER_BLOCK, &READ_DURATION_PER_BLOCK_HIST),
            ms_gauge_and_hist_panel(&COMPUTE_DURATION_PER_BLOCK, &COMPUTE_DURATION_PER_BLOCK_HIST),
            ms_gauge_and_hist_panel(&WRITE_DURATION_PER_BLOCK, &WRITE_DURATION_PER_BLOCK_HIST),
        ],
    )
}
