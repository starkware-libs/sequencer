//! Interface for handling state commitments.
//! Import [`CommitmentOutputStorageReader`] and [`CommitmentOutputStorageWriter`] to read and write
//! data related to state commitments using a [`StorageTxn`].

use starknet_api::block::BlockNumber;
use starknet_api::hash::CommitmentOutput;

use crate::db::table_types::Table;
use crate::db::{TransactionKind, RW};
use crate::{StorageResult, StorageTxn};

/// Interface for reading commitment outputs.
pub trait CommitmentOutputStorageReader {
    /// Returns the commitment output corresponding to the block number.
    /// Returns `None` if the block number is not found.
    fn get_commitment_output(
        &self,
        block_number: &BlockNumber,
    ) -> StorageResult<Option<CommitmentOutput>>;
}

/// Interface for writing state commitments.
pub trait CommitmentOutputStorageWriter
where
    Self: Sized,
{
    /// Inserts the commitment output corresponding to the given block number.
    /// An error is returned if the commitment output already exists.
    fn set_commitment_output(
        self,
        block_number: &BlockNumber,
        commitment_output: CommitmentOutput,
    ) -> StorageResult<Self>;

    /// Revert the commitment output corresponding to the given block number.
    fn revert_commitment_output(self, block_number: &BlockNumber) -> StorageResult<Self>;
}

impl<Mode: TransactionKind> CommitmentOutputStorageReader for StorageTxn<'_, Mode> {
    fn get_commitment_output(
        &self,
        block_number: &BlockNumber,
    ) -> StorageResult<Option<CommitmentOutput>> {
        let table = self.open_table(&self.tables.state_roots)?;
        Ok(table.get(&self.txn, block_number)?)
    }
}

impl CommitmentOutputStorageWriter for StorageTxn<'_, RW> {
    fn set_commitment_output(
        self,
        block_number: &BlockNumber,
        commitment_output: CommitmentOutput,
    ) -> StorageResult<Self> {
        let table = self.open_table(&self.tables.state_roots)?;
        table.upsert(&self.txn, block_number, &commitment_output)?;
        Ok(self)
    }

    fn revert_commitment_output(self, target_block_number: &BlockNumber) -> StorageResult<Self> {
        let table = self.open_table(&self.tables.state_roots)?;
        table.delete(&self.txn, target_block_number)?;
        Ok(self)
    }
}
