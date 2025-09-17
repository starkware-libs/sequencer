use apollo_storage::metrics::{
    BATCHER_STORAGE_OPEN_READ_TRANSACTIONS,
    CLASS_MANAGER_STORAGE_OPEN_READ_TRANSACTIONS,
    STORAGE_APPEND_THIN_STATE_DIFF_LATENCY,
    STORAGE_COMMIT_LATENCY,
    SYNC_STORAGE_OPEN_READ_TRANSACTIONS,
};

use crate::dashboard::{Panel, PanelType, Row, Unit};

fn get_storage_append_thin_state_diff_latency() -> Panel {
    Panel::from_hist(
        &STORAGE_APPEND_THIN_STATE_DIFF_LATENCY,
        "Append Thin State Diff Latency",
        "Latency to append thin state diff in storage",
    )
    .with_unit(Unit::Seconds)
}
fn get_storage_commit_latency() -> Panel {
    Panel::from_hist(
        &STORAGE_COMMIT_LATENCY,
        "Storage Commit Latency",
        "Latency to commit changes in storage",
    )
    .with_unit(Unit::Seconds)
}
fn get_sync_storage_open_read_transactions() -> Panel {
    Panel::from_gauge(&SYNC_STORAGE_OPEN_READ_TRANSACTIONS, PanelType::TimeSeries)
}
fn get_batcher_storage_open_read_transactions() -> Panel {
    Panel::from_gauge(&BATCHER_STORAGE_OPEN_READ_TRANSACTIONS, PanelType::TimeSeries)
}
fn get_class_manager_storage_open_read_transactions() -> Panel {
    Panel::from_gauge(&CLASS_MANAGER_STORAGE_OPEN_READ_TRANSACTIONS, PanelType::TimeSeries)
}

pub(crate) fn get_storage_row() -> Row {
    Row::new(
        "Storage",
        vec![
            get_storage_append_thin_state_diff_latency(),
            get_storage_commit_latency(),
            get_sync_storage_open_read_transactions(),
            get_batcher_storage_open_read_transactions(),
            get_class_manager_storage_open_read_transactions(),
        ],
    )
}
