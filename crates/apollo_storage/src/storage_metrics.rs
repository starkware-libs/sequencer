//! module for metrics utilities.
#[cfg(test)]
#[path = "storage_metrics_test.rs"]
mod storage_metrics_test;

use metrics::{counter, gauge};
use tracing::debug;

use crate::{StorageReader, StorageResult};

// TODO(dvir): add storage metrics names to this module.

// TODO(dvir): consider adding storage size metrics.
// TODO(dvir): relocate all the storage metrics in one module and export them (also in other
// crates).
/// Updates storage metrics about the state of the storage.
#[allow(clippy::as_conversions)]
pub fn update_storage_metrics(reader: &StorageReader) -> StorageResult<()> {
    debug!("updating storage metrics");
    let state = reader.shared_state.lock().unwrap();
    gauge!("storage_free_pages_number").set(state.db_writer.get_free_pages()? as f64);
    let info = state.db_writer.get_db_info()?;
    counter!("storage_last_page_number")
        .absolute(u64::try_from(info.last_pgno()).expect("usize should fit in u64"));
    counter!("storage_last_transaction_index")
        .absolute(u64::try_from(info.last_txnid()).expect("usize should fit in u64"));
    Ok(())
}
