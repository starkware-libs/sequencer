//! Storage for the per-block OS-input commitment infos (state-trie commitment data for the OS).
//!
//! Persists the already-compressed `CompressedStateCommitmentInfos` the committer produces.

use starknet_api::block::BlockNumber;
pub use starknet_committer::patricia_merkle_tree::types::CompressedStateCommitmentInfos;

#[cfg(test)]
#[path = "state_commitment_infos_test.rs"]
mod state_commitment_infos_test;

use crate::db::serialization::{StorageSerde, StorageSerdeError};
use crate::db::table_types::Table;
use crate::db::{TransactionKind, RW};
use crate::mmap_file::LocationInFile;
use crate::{OffsetKind, StorageResult, StorageTransaction};

// Stores the raw compressed bytes.
impl StorageSerde for CompressedStateCommitmentInfos {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
        self.0.serialize_into(res)
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        Some(Self(Vec::<u8>::deserialize_from(bytes)?))
    }
}

/// Interface for reading the OS-input commitment infos from storage.
pub trait StateCommitmentInfosStorageReader<Mode: TransactionKind> {
    /// Returns the compressed commitment infos for the given block, or `None` if not stored (e.g.
    /// the block was added via sync, or its commitment infos were reverted).
    fn get_state_commitment_infos(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<CompressedStateCommitmentInfos>>;

    /// Returns whether the compressed commitment infos for the given block are stored, without
    /// reading the stored blob. `false` carries the same meaning as `get_state_commitment_infos`
    /// returning `None`; unlike `get_state_commitment_infos`, this does not detect a stored entry
    /// whose blob is unreadable (e.g. on-disk corruption), since that would require reading the
    /// blob and defeat the purpose of this cheaper check.
    fn has_state_commitment_infos(&self, block_number: BlockNumber) -> StorageResult<bool>;
}

/// Interface for writing the OS-input commitment infos to storage.
pub trait StateCommitmentInfosStorageWriter
where
    Self: Sized,
{
    /// Appends the compressed commitment infos for the given block to storage.
    fn append_state_commitment_infos(
        self,
        block_number: BlockNumber,
        state_commitment_infos: &CompressedStateCommitmentInfos,
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
    ) -> StorageResult<Option<CompressedStateCommitmentInfos>> {
        let Some(location) = self.state_commitment_infos_location(block_number)? else {
            return Ok(None);
        };
        Ok(Some(self.file_handlers().get_state_commitment_infos_unchecked(location)?))
    }

    fn has_state_commitment_infos(&self, block_number: BlockNumber) -> StorageResult<bool> {
        Ok(self.state_commitment_infos_location(block_number)?.is_some())
    }
}

trait StateCommitmentInfosLocationReader {
    /// Looks up the stored location of the given block's compressed commitment infos, without
    /// reading the blob itself.
    fn state_commitment_infos_location(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<LocationInFile>>;
}

impl<T: StorageTransaction> StateCommitmentInfosLocationReader for T {
    fn state_commitment_infos_location(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<LocationInFile>> {
        let table = self.open_table(&self.tables().state_commitment_infos)?;
        Ok(table.get(self.txn(), &block_number)?)
    }
}

impl<T: StorageTransaction<Mode = RW>> StateCommitmentInfosStorageWriter for T {
    fn append_state_commitment_infos(
        self,
        block_number: BlockNumber,
        state_commitment_infos: &CompressedStateCommitmentInfos,
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
