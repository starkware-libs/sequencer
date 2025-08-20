use apollo_storage::metrics::{STORAGE_APPEND_THIN_STATE_DIFF_LATENCY, STORAGE_COMMIT_LATENCY};

use crate::dashboard::{Panel, PanelType, Row};

fn get_storage_append_thin_state_diff_latency() -> Panel {
    Panel::from_hist(STORAGE_APPEND_THIN_STATE_DIFF_LATENCY, PanelType::TimeSeries)
}
fn get_storage_commit_latency() -> Panel {
    Panel::from_hist(STORAGE_COMMIT_LATENCY, PanelType::TimeSeries)
}

pub(crate) fn get_storage_row() -> Row {
    Row::new(
        "Storage",
        vec![get_storage_append_thin_state_diff_latency(), get_storage_commit_latency()],
    )
}
