//! Interface for handling partial block hashes.
//! Import [`PartialBlockHashStorageReader`] and [`PartialBlockHashStorageWriter`] to read and write
//! data related to partial block hashes using a [`StorageTxn`].

use starknet_api::block::BlockNumber;
use starknet_api::block_hash::block_hash_calculator::PartialBlockHash;

use crate::db::table_types::Table;
use crate::db::{TransactionKind, RW};
use crate::{StorageResult, StorageTxn};

/// Interface for reading executable class hashes.
pub trait PartialBlockHashStorageReader {
    /// Returns the partial block hash corresponding to the given block number.
    /// Returns `None` if the block number is not found.
    fn get_partial_block_hash(
        &self,
        block_number: &BlockNumber,
    ) -> StorageResult<Option<PartialBlockHash>>;
}

/// Interface for writing partial block hashes.
pub trait PartialBlockHashStorageWriter
where
    Self: Sized,
{
    /// Inserts the partial block hash corresponding to the given block number.
    /// An error is returned if the block number already exists.
    fn set_partial_block_hash(
        self,
        block_number: &BlockNumber,
        partial_block_hash: &PartialBlockHash,
    ) -> StorageResult<Self>;

    /// Reverts the partial block hash corresponding to the given block number.
    fn revert_partial_block_hash(self, block_number: &BlockNumber) -> StorageResult<Self>;
}

impl<Mode: TransactionKind> PartialBlockHashStorageReader for StorageTxn<'_, Mode> {
    fn get_partial_block_hash(
        &self,
        block_number: &BlockNumber,
    ) -> StorageResult<Option<PartialBlockHash>> {
        let table = self.open_table(&self.tables.partial_block_hashes)?;
        Ok(table.get(&self.txn, block_number)?)
    }
}

impl PartialBlockHashStorageWriter for StorageTxn<'_, RW> {
    fn set_partial_block_hash(
        self,
        block_number: &BlockNumber,
        partial_block_hash: &PartialBlockHash,
    ) -> StorageResult<Self> {
        let table = self.open_table(&self.tables.partial_block_hashes)?;
        table.insert(&self.txn, block_number, partial_block_hash)?;
        Ok(self)
    }

    fn revert_partial_block_hash(self, block_number: &BlockNumber) -> StorageResult<Self> {
        let table = self.open_table(&self.tables.partial_block_hashes)?;
        table.delete(&self.txn, block_number)?;
        Ok(self)
    }
}
