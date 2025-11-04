//! Interface for handling block hashes.
//! Import [`BlockHashStorageReader`] and [`BlockHashStorageWriter`] to read and write
//! data related to block hashes using a [`StorageTxn`].

use starknet_api::block::{BlockHash, BlockNumber};

use crate::db::table_types::Table;
use crate::db::{TransactionKind, RW};
use crate::{StorageResult, StorageTxn};

/// Interface for reading block hashes.
pub trait BlockHashStorageReader {
    /// Returns the block hash corresponding to the block number.
    /// Returns `None` if the block number is not found.
    fn get_block_hash(&self, block_number: &BlockNumber) -> StorageResult<Option<BlockHash>>;
}

/// Interface for writing block hashes.
pub trait BlockHashStorageWriter
where
    Self: Sized,
{
    /// Inserts the block hash corresponding to the given block number.
    /// An error is returned if the block hash already exists.
    fn set_block_hash(
        self,
        block_number: &BlockNumber,
        block_hash: BlockHash,
    ) -> StorageResult<Self>;

    /// Revert the block hash corresponding to the given block number.
    fn revert_block_hash(self, block_number: &BlockNumber) -> StorageResult<Self>;
}

impl<Mode: TransactionKind> BlockHashStorageReader for StorageTxn<'_, Mode> {
    fn get_block_hash(&self, block_number: &BlockNumber) -> StorageResult<Option<BlockHash>> {
        let table = self.open_table(&self.tables.block_hashes)?;
        Ok(table.get(&self.txn, block_number)?)
    }
}

impl BlockHashStorageWriter for StorageTxn<'_, RW> {
    fn set_block_hash(
        self,
        block_number: &BlockNumber,
        block_hash: BlockHash,
    ) -> StorageResult<Self> {
        let table = self.open_table(&self.tables.block_hashes)?;
        table.upsert(&self.txn, block_number, &block_hash)?;
        Ok(self)
    }

    fn revert_block_hash(self, target_block_number: &BlockNumber) -> StorageResult<Self> {
        let table = self.open_table(&self.tables.block_hashes)?;
        table.delete(&self.txn, target_block_number)?;
        Ok(self)
    }
}
