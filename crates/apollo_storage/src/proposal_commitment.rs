//! Interface for handling proposal commitments.
//! Import [`ProposalCommitmentStorageReader`] and [`ProposalCommitmentStorageWriter`] to read and
//! write data related to proposal commitments using a [`StorageTxn`].

use starknet_api::block::BlockNumber;
use starknet_api::block_hash::block_hash_calculator::PartialBlockHash;

use crate::db::table_types::Table;
use crate::db::{TransactionKind, RW};
use crate::{StorageResult, StorageTxn};

/// Interface for reading proposal commitments.
pub trait ProposalCommitmentStorageReader {
    /// Returns the proposal commitment corresponding to the given block number.
    /// Returns `None` if the block number is not found.
    fn get_proposal_commitment(
        &self,
        block_number: &BlockNumber,
    ) -> StorageResult<Option<PartialBlockHash>>;
}

/// Interface for writing proposal commitments.
pub trait ProposalCommitmentStorageWriter
where
    Self: Sized,
{
    /// Inserts the proposal commitment corresponding to the given block number.
    /// An error is returned if the block number already exists.
    fn set_proposal_commitment(
        self,
        block_number: &BlockNumber,
        proposal_commitment: &PartialBlockHash,
    ) -> StorageResult<Self>;

    /// Reverts the proposal commitment corresponding to the given block number.
    fn revert_proposal_commitment(self, block_number: &BlockNumber) -> StorageResult<Self>;
}

impl<Mode: TransactionKind> ProposalCommitmentStorageReader for StorageTxn<'_, Mode> {
    fn get_proposal_commitment(
        &self,
        block_number: &BlockNumber,
    ) -> StorageResult<Option<PartialBlockHash>> {
        let table = self.open_table(&self.tables.proposal_commitments)?;
        Ok(table.get(&self.txn, block_number)?)
    }
}

impl ProposalCommitmentStorageWriter for StorageTxn<'_, RW> {
    fn set_proposal_commitment(
        self,
        block_number: &BlockNumber,
        proposal_commitment: &PartialBlockHash,
    ) -> StorageResult<Self> {
        let table = self.open_table(&self.tables.proposal_commitments)?;
        table.insert(&self.txn, block_number, proposal_commitment)?;
        Ok(self)
    }

    fn revert_proposal_commitment(self, block_number: &BlockNumber) -> StorageResult<Self> {
        let table = self.open_table(&self.tables.proposal_commitments)?;
        table.delete(&self.txn, block_number)?;
        Ok(self)
    }
}
