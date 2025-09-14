use apollo_storage::metrics::{STORAGE_APPEND_THIN_STATE_DIFF_LATENCY, STORAGE_COMMIT_LATENCY};

use crate::dashboard::{Panel, Row};

fn get_storage_append_thin_state_diff_latency() -> Panel {
    Panel::from(&STORAGE_APPEND_THIN_STATE_DIFF_LATENCY)
}
fn get_storage_commit_latency() -> Panel {
    Panel::from(&STORAGE_COMMIT_LATENCY)
}

pub(crate) fn get_storage_row() -> Row {
    Row::new(
        "Storage",
        vec![get_storage_append_thin_state_diff_latency(), get_storage_commit_latency()],
    )
}
