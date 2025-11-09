//! Interface for handling partial block hashes.
//! Import [`PartialBlockHashComponentsStorageReader`] and
//! [`PartialBlockHashComponentsStorageWriter`] to read and write data related to partial block
//! hashes using a [`StorageTxn`].

use starknet_api::block::BlockNumber;
use starknet_api::block_hash::block_hash_calculator::PartialBlockHashComponents;

use crate::db::table_types::Table;
use crate::db::{TransactionKind, RW};
use crate::{StorageResult, StorageTxn};

/// Interface for reading partial block hashes.
pub trait PartialBlockHashComponentsStorageReader {
    /// Returns the partial block hash corresponding to the given block number.
    /// Returns `None` if the block number is not found.
    fn get_partial_block_hash_components(
        &self,
        block_number: &BlockNumber,
    ) -> StorageResult<Option<PartialBlockHashComponents>>;
}

/// Interface for writing partial block hashes.
pub trait PartialBlockHashComponentsStorageWriter
where
    Self: Sized,
{
    /// Inserts the partial block hash corresponding to the given block number.
    /// An error is returned if the block number already exists.
    fn set_partial_block_hash_components(
        self,
        block_number: &BlockNumber,
        partial_block_hash: &PartialBlockHashComponents,
    ) -> StorageResult<Self>;

    /// Reverts the partial block hash corresponding to the given block number.
    fn revert_partial_block_hash_components(
        self,
        block_number: &BlockNumber,
    ) -> StorageResult<Self>;
}

impl<Mode: TransactionKind> PartialBlockHashComponentsStorageReader for StorageTxn<'_, Mode> {
    fn get_partial_block_hash_components(
        &self,
        block_number: &BlockNumber,
    ) -> StorageResult<Option<PartialBlockHashComponents>> {
        let table = self.open_table(&self.tables.partial_block_hashes_components)?;
        Ok(table.get(&self.txn, block_number)?)
    }
}

impl PartialBlockHashComponentsStorageWriter for StorageTxn<'_, RW> {
    fn set_partial_block_hash_components(
        self,
        block_number: &BlockNumber,
        partial_block_hash: &PartialBlockHashComponents,
    ) -> StorageResult<Self> {
        let table = self.open_table(&self.tables.partial_block_hashes_components)?;
        table.insert(&self.txn, block_number, partial_block_hash)?;
        Ok(self)
    }

    fn revert_partial_block_hash_components(
        self,
        block_number: &BlockNumber,
    ) -> StorageResult<Self> {
        let table = self.open_table(&self.tables.partial_block_hashes_components)?;
        table.delete(&self.txn, block_number)?;
        Ok(self)
    }
}
