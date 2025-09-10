use apollo_storage::metrics::{
    STORAGE_APPEND_THIN_STATE_DIFF_LATENCY,
    STORAGE_COMMIT_LATENCY,
    STORAGE_OPEN_BATCHER_READ_TRANSACTIONS,
    STORAGE_OPEN_CLASS_MANAGER_READ_TRANSACTIONS,
    STORAGE_OPEN_SYNC_READ_TRANSACTIONS,
};

use crate::dashboard::{Panel, Row};

fn get_storage_append_thin_state_diff_latency() -> Panel {
    Panel::from(&STORAGE_APPEND_THIN_STATE_DIFF_LATENCY)
}
fn get_storage_commit_latency() -> Panel {
    Panel::from(&STORAGE_COMMIT_LATENCY)
}
fn get_storage_open_sync_read_transactions() -> Panel {
    Panel::from(&STORAGE_OPEN_SYNC_READ_TRANSACTIONS)
}
fn get_storage_open_batcher_read_transactions() -> Panel {
    Panel::from(&STORAGE_OPEN_BATCHER_READ_TRANSACTIONS)
}
fn get_storage_open_class_manager_read_transactions() -> Panel {
    Panel::from(&STORAGE_OPEN_CLASS_MANAGER_READ_TRANSACTIONS)
}

pub(crate) fn get_storage_row() -> Row {
    Row::new(
        "Storage",
        vec![
            get_storage_append_thin_state_diff_latency(),
            get_storage_commit_latency(),
            get_storage_open_sync_read_transactions(),
            get_storage_open_batcher_read_transactions(),
            get_storage_open_class_manager_read_transactions(),
        ],
    )
}
