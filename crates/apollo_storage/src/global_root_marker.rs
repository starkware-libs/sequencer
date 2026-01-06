//! Interface for handling global root markers.
//! The global root marker is the highest block number for which the global root is stored.
//! Import [`GlobalRootMarkerStorageReader`] and [`GlobalRootMarkerStorageWriter`] to
//! read and write data related to global root markers using a [`StorageTxn`].

use starknet_api::block::BlockNumber;

use crate::db::table_types::Table;
use crate::db::{TransactionKind, RW};
use crate::{MarkerKind, StorageError, StorageResult, StorageTxn};

/// Interface for reading global root markers.
pub trait GlobalRootMarkerStorageReader {
    /// Returns the global root marker.
    fn get_global_root_marker(&self) -> StorageResult<BlockNumber>;
}

/// Interface for writing global root markers.
pub trait GlobalRootMarkerStorageWriter
where
    Self: Sized,
{
    /// Increments the global root marker if it matches the expected marker.
    /// Otherwise, returns an error.
    fn checked_increment_global_root_marker(
        self,
        expected_marker: BlockNumber,
    ) -> StorageResult<Self>;
}

impl<Mode: TransactionKind> GlobalRootMarkerStorageReader for StorageTxn<'_, Mode> {
    fn get_global_root_marker(&self) -> StorageResult<BlockNumber> {
        let table = self.open_table(&self.tables.markers)?;
        Ok(table.get(&self.txn, &MarkerKind::GlobalRoot)?.unwrap_or_default())
    }
}

impl GlobalRootMarkerStorageWriter for StorageTxn<'_, RW> {
    fn checked_increment_global_root_marker(
        self,
        expected_marker: BlockNumber,
    ) -> StorageResult<Self> {
        let current_marker = self.get_global_root_marker()?;
        if current_marker != expected_marker {
            return Err(StorageError::MarkerMismatch {
                found: current_marker,
                expected: expected_marker,
            });
        }
        let markers_table = self.open_table(&self.tables.markers)?;
        markers_table.upsert(
            &self.txn,
            &MarkerKind::GlobalRoot,
            &current_marker.unchecked_next(),
        )?;
        Ok(self)
    }
}
