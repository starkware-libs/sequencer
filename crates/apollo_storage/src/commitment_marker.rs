//! Interface for handling commitment markers.
//! The commitment marker is the highest block number for which both the block hash and state root
//! are stored. Import [`CommitmentMarkerStorageReader`] and [`CommitmentMarkerStorageWriter`] to
//! read and write data related to commitment markers using a [`StorageTxn`].

use starknet_api::block::BlockNumber;

use crate::db::table_types::Table;
use crate::db::{TransactionKind, RW};
use crate::{MarkerKind, StorageResult, StorageTxn};

/// Interface for reading commitment markers.
pub trait CommitmentMarkerStorageReader {
    /// Returns the commitment marker.
    fn get_commitment_marker(&self) -> StorageResult<BlockNumber>;
}

/// Interface for writing commitment markers.
pub trait CommitmentMarkerStorageWriter
where
    Self: Sized,
{
    /// Updates the commitment marker to the given block number.
    fn set_commitment_marker(self, marker: BlockNumber) -> StorageResult<Self>;

    /// Increments the commitment marker.
    fn increment_commitment_marker(self) -> StorageResult<Self>;
}

impl<Mode: TransactionKind> CommitmentMarkerStorageReader for StorageTxn<'_, Mode> {
    fn get_commitment_marker(&self) -> StorageResult<BlockNumber> {
        let table = self.open_table(&self.tables.markers)?;
        Ok(table.get(&self.txn, &MarkerKind::Commitment)?.unwrap_or_default())
    }
}

impl CommitmentMarkerStorageWriter for StorageTxn<'_, RW> {
    fn set_commitment_marker(self, marker: BlockNumber) -> StorageResult<Self> {
        let table = self.open_table(&self.tables.markers)?;
        table.upsert(&self.txn, &MarkerKind::Commitment, &marker)?;
        Ok(self)
    }

    fn increment_commitment_marker(self) -> StorageResult<Self> {
        let current_marker = self.get_commitment_marker()?;
        self.set_commitment_marker(current_marker.unchecked_next())
    }
}
