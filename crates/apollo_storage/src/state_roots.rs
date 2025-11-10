//! Interface for handling state roots commitments.
//! Import [`StateRootsStorageReader`] and [`StateRootsStorageWriter`] to read and write
//! data related to state roots commitments using a [`StorageTxn`].

use starknet_api::block::BlockNumber;
use starknet_api::hash::StateRoots;

use crate::db::table_types::Table;
use crate::db::{TransactionKind, RW};
use crate::{StorageResult, StorageTxn};

/// Interface for reading state roots.
pub trait StateRootsStorageReader {
    /// Returns the state roots corresponding to the block number.
    /// Returns `None` if the block number is not found.
    fn get_state_roots(&self, block_number: &BlockNumber) -> StorageResult<Option<StateRoots>>;
}

/// Interface for writing state roots.
pub trait StateRootsStorageWriter
where
    Self: Sized,
{
    /// Inserts the state roots corresponding to the given block number.
    /// An error is returned if the state roots already exists.
    fn set_state_roots(
        self,
        block_number: &BlockNumber,
        state_roots: StateRoots,
    ) -> StorageResult<Self>;

    /// Revert the state roots corresponding to the given block number.
    fn revert_state_roots(self, block_number: &BlockNumber) -> StorageResult<Self>;
}

impl<Mode: TransactionKind> StateRootsStorageReader for StorageTxn<'_, Mode> {
    fn get_state_roots(&self, block_number: &BlockNumber) -> StorageResult<Option<StateRoots>> {
        let table = self.open_table(&self.tables.state_roots)?;
        Ok(table.get(&self.txn, block_number)?)
    }
}

impl StateRootsStorageWriter for StorageTxn<'_, RW> {
    fn set_state_roots(
        self,
        block_number: &BlockNumber,
        state_roots: StateRoots,
    ) -> StorageResult<Self> {
        let table = self.open_table(&self.tables.state_roots)?;
        table.upsert(&self.txn, block_number, &state_roots)?;
        Ok(self)
    }

    fn revert_state_roots(self, target_block_number: &BlockNumber) -> StorageResult<Self> {
        let table = self.open_table(&self.tables.state_roots)?;
        table.delete(&self.txn, target_block_number)?;
        Ok(self)
    }
}
