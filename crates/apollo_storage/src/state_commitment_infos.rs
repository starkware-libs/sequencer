//! Storage for the per-block OS-input commitment infos (state-trie commitment data for the OS).
//!
//! Stored as the already-compressed string (`base64(zstd(serde_json(..)))`) the committer produces,
//! so this path persists the witness verbatim without (de)serializing `StateCommitmentInfos`.

use starknet_api::block::BlockNumber;

#[cfg(test)]
#[path = "state_commitment_infos_test.rs"]
mod state_commitment_infos_test;

use crate::db::table_types::Table;
use crate::db::{TransactionKind, RW};
use crate::{OffsetKind, StorageResult, StorageTransaction};

/// Interface for reading the OS-input commitment infos from storage.
pub trait StateCommitmentInfosStorageReader<Mode: TransactionKind> {
    /// Returns the compressed commitment infos for the given block, or `None` if not stored.
    fn get_state_commitment_infos(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<String>>;
}

/// Interface for writing the OS-input commitment infos to storage.
pub trait StateCommitmentInfosStorageWriter
where
    Self: Sized,
{
    /// Appends the compressed commitment infos for the given block to storage.
    // Takes `&String` (not `&str`) because the mmap file handler appends `&V::Value`; `&str` would
    // force an extra copy of the (potentially multi-MB) compressed witness.
    #[allow(clippy::ptr_arg)]
    fn append_state_commitment_infos(
        self,
        block_number: BlockNumber,
        state_commitment_infos: &String,
    ) -> StorageResult<Self>;

    /// Removes the commitment infos for the given block from storage.
    /// If no entry exists for the block, returns without error.
    fn revert_state_commitment_infos(self, block_number: BlockNumber) -> StorageResult<Self>;
}

impl<T: StorageTransaction> StateCommitmentInfosStorageReader<<T as StorageTransaction>::Mode>
    for T
{
    fn get_state_commitment_infos(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<String>> {
        let table = self.open_table(&self.tables().state_commitment_infos)?;
        let Some(location) = table.get(self.txn(), &block_number)? else {
            return Ok(None);
        };
        Ok(Some(self.file_handlers().get_state_commitment_infos_unchecked(location)?))
    }
}

impl<T: StorageTransaction<Mode = RW>> StateCommitmentInfosStorageWriter for T {
    #[allow(clippy::ptr_arg)]
    fn append_state_commitment_infos(
        self,
        block_number: BlockNumber,
        state_commitment_infos: &String,
    ) -> StorageResult<Self> {
        let file_offset_table = self.open_table(&self.tables().file_offsets)?;
        let state_commitment_infos_table =
            self.open_table(&self.tables().state_commitment_infos)?;

        let location = self.file_handlers().append_state_commitment_infos(state_commitment_infos);
        state_commitment_infos_table.upsert(self.txn(), &block_number, &location)?;
        file_offset_table.upsert(
            self.txn(),
            &OffsetKind::StateCommitmentInfos,
            &location.next_offset(),
        )?;

        Ok(self)
    }

    fn revert_state_commitment_infos(self, block_number: BlockNumber) -> StorageResult<Self> {
        let state_commitment_infos_table =
            self.open_table(&self.tables().state_commitment_infos)?;
        state_commitment_infos_table.delete(self.txn(), &block_number)?;
        Ok(self)
    }
}
