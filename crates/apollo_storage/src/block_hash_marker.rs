//! Interface for handling block hash markers.
//! The block hash marker is the highest block number for which the block hash is stored.
//! Import [`BlockHashMarkerStorageReader`] and [`BlockHashMarkerStorageWriter`] to
//! read and write data related to block hash markers using a [`StorageTxn`].

use starknet_api::block::BlockNumber;

use crate::db::table_types::Table;
use crate::db::{TransactionKind, RW};
use crate::{MarkerKind, StorageResult, StorageTxn};

/// Interface for reading block hash markers.
pub trait BlockHashMarkerStorageReader {
    /// Returns the block hash marker.
    fn get_block_hash_marker(&self) -> StorageResult<BlockNumber>;
}

/// Interface for writing block hash markers.
pub trait BlockHashMarkerStorageWriter
where
    Self: Sized,
{
    /// Increments the block hash marker.
    fn increment_block_hash_marker(self) -> StorageResult<Self>;
}

impl<Mode: TransactionKind> BlockHashMarkerStorageReader for StorageTxn<'_, Mode> {
    fn get_block_hash_marker(&self) -> StorageResult<BlockNumber> {
        let table = self.open_table(&self.tables.markers)?;
        Ok(table.get(&self.txn, &MarkerKind::BlockHash)?.unwrap_or_default())
    }
}

impl BlockHashMarkerStorageWriter for StorageTxn<'_, RW> {
    fn increment_block_hash_marker(self) -> StorageResult<Self> {
        let current_marker = self.get_block_hash_marker()?;
        let markers_table = self.open_table(&self.tables.markers)?;
        markers_table.upsert(
            &self.txn,
            &MarkerKind::BlockHash,
            &current_marker.unchecked_next(),
        )?;
        Ok(self)
    }
}
