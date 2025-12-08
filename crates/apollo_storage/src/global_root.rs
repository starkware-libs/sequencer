//! Interface for handling state roots commitments.
//! Import [`StateRootsStorageReader`] and [`StateRootsStorageWriter`] to read and write
//! data related to state roots commitments using a [`StorageTxn`].

use starknet_api::block::BlockNumber;
use starknet_api::core::GlobalRoot;

use crate::db::table_types::Table;
use crate::db::{TransactionKind, RW};
use crate::{StorageResult, StorageTxn};

/// Interface for reading global root.
pub trait GlobalRootStorageReader {
    /// Returns the global root corresponding to the block number.
    /// Returns `None` if the block number is not found.
    fn get_global_root(&self, block_number: &BlockNumber) -> StorageResult<Option<GlobalRoot>>;
}

/// Interface for writing global root.
pub trait GlobalRootStorageWriter
where
    Self: Sized,
{
    /// Inserts the global root corresponding to the given block number.
    /// An error is returned if there is already a global root for given block number.
    fn set_global_root(
        self,
        block_number: &BlockNumber,
        global_root: GlobalRoot,
    ) -> StorageResult<Self>;

    /// Revert the global root corresponding to the given block number.
    fn revert_global_root(self, block_number: &BlockNumber) -> StorageResult<Self>;
}

impl<Mode: TransactionKind> GlobalRootStorageReader for StorageTxn<'_, Mode> {
    fn get_global_root(&self, block_number: &BlockNumber) -> StorageResult<Option<GlobalRoot>> {
        let table = self.open_table(&self.tables.global_root)?;
        Ok(table.get(&self.txn, block_number)?)
    }
}

impl GlobalRootStorageWriter for StorageTxn<'_, RW> {
    fn set_global_root(
        self,
        block_number: &BlockNumber,
        global_root: GlobalRoot,
    ) -> StorageResult<Self> {
        let table = self.open_table(&self.tables.global_root)?;
        table.upsert(&self.txn, block_number, &global_root)?;
        Ok(self)
    }

    fn revert_global_root(self, target_block_number: &BlockNumber) -> StorageResult<Self> {
        let table = self.open_table(&self.tables.global_root)?;
        table.delete(&self.txn, target_block_number)?;
        Ok(self)
    }
}
