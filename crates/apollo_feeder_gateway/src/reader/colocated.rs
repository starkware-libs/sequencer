use std::sync::Arc;

use apollo_storage::header::HeaderStorageReader;
use apollo_storage::StorageReader;
use async_trait::async_trait;
use starknet_api::block::BlockHeader;

use crate::reader::executor::ReadExecutor;
use crate::reader::{internal_error, ChainDataReader, FgResult};

#[cfg(test)]
#[path = "colocated_test.rs"]
mod colocated_test;

/// A [`ChainDataReader`] that reads directly from a co-located `apollo_storage::StorageReader`.
///
/// Every read is dispatched through the bounded [`ReadExecutor`] so MDBX reads run in parallel on
/// the blocking pool (MVCC permits many concurrent read transactions) without blocking the async
/// reactor. This mirrors how `apollo_rpc` is handed a `StorageReader` directly, but with the read
/// pool bound applied.
pub struct ColocatedStorageReader {
    storage_reader: StorageReader,
    executor: Arc<ReadExecutor>,
}

impl ColocatedStorageReader {
    pub fn new(storage_reader: StorageReader, executor: Arc<ReadExecutor>) -> Self {
        Self { storage_reader, executor }
    }
}

#[async_trait]
impl ChainDataReader for ColocatedStorageReader {
    async fn latest_block_header(&self) -> FgResult<Option<BlockHeader>> {
        // `StorageReader` is an `Arc<Environment>` internally, so cloning is cheap and shares the
        // single MDBX environment.
        let storage_reader = self.storage_reader.clone();
        self.executor
            .run(move || {
                let txn = storage_reader.begin_ro_txn().map_err(internal_error)?;
                let header_marker = txn.get_header_marker().map_err(internal_error)?;
                // The header marker points one past the latest stored block; `prev()` is `None`
                // only when no blocks have been synced yet.
                let Some(latest_block_number) = header_marker.prev() else {
                    return Ok(None);
                };
                txn.get_block_header(latest_block_number).map_err(internal_error)
            })
            .await?
    }
}
