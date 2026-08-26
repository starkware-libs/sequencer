//! Storage for the per-block OS-input commitment infos (state-trie commitment data for the OS).
//!
//! Persists the already-compressed `CompressedStateCommitmentInfos` the committer produces. The
//! compressed bytes are appended to the commitment infos data file, and the pointers table maps
//! each block to the location of its infos in that file. Pruning removes a block's pointer, then
//! releases the data its pointer pointed at.

use starknet_api::block::BlockNumber;
pub use starknet_committer::patricia_merkle_tree::types::{
    CompressedPayload,
    CompressedStateCommitmentInfos,
    STATE_COMMITMENT_INFOS_VERSION,
};

#[cfg(test)]
#[path = "state_commitment_infos_test.rs"]
mod state_commitment_infos_test;

use crate::db::serialization::{StorageSerde, StorageSerdeError};
use crate::db::table_types::{DbCursorTrait, Table};
use crate::db::{TransactionKind, RW};
use crate::mmap_file::LocationInFile;
use crate::{OffsetKind, StorageResult, StorageTransaction, StorageWriter};

/// The outcome of pruning state commitment infos pointers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PrunedStateCommitmentInfosPointers {
    /// One past the last removed height; no infos are stored below it.
    pub new_lower_bound: BlockNumber,
    /// The data file offset just past the infos of the last removed pointer; the data below it is
    /// pointed at by no pointer, and can be released.
    pub data_end_offset: usize,
}

impl StorageSerde for CompressedStateCommitmentInfos {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
        self.version.serialize_into(res)?;
        self.payload.0.serialize_into(res)
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        Some(Self {
            version: u8::deserialize_from(bytes)?,
            payload: CompressedPayload(Vec::<u8>::deserialize_from(bytes)?),
        })
    }
}

/// Interface for reading the OS-input commitment infos from storage.
pub trait StateCommitmentInfosStorageReader<Mode: TransactionKind> {
    /// Returns the compressed commitment infos for the given block, or `None` if not stored.
    fn get_state_commitment_infos(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<CompressedStateCommitmentInfos>>;

    /// Returns whether the compressed commitment infos for the given block are stored, without
    /// reading the stored blob.
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

    /// Removes the pointers to the commitment infos of the lowest stored heights below
    /// `prune_below`, at most `max_deletions` of them. Returns `None` if nothing was removed. The
    /// data they pointed at stays in the data file until released with
    /// [`StorageWriter::prune_state_commitment_infos_data`].
    fn prune_state_commitment_infos_pointers(
        self,
        prune_below: BlockNumber,
        max_deletions: usize,
    ) -> StorageResult<(Self, Option<PrunedStateCommitmentInfosPointers>)>;
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

    fn prune_state_commitment_infos_pointers(
        self,
        prune_below: BlockNumber,
        max_deletions: usize,
    ) -> StorageResult<(Self, Option<PrunedStateCommitmentInfosPointers>)> {
        let state_commitment_infos_table =
            self.open_table(&self.tables().state_commitment_infos)?;
        let mut cursor = state_commitment_infos_table.cursor(self.txn())?;
        let mut entries_to_delete = Vec::new();
        let mut entry = cursor.lower_bound(&BlockNumber(0))?;
        while let Some((block_number, location)) = entry {
            if block_number >= prune_below || entries_to_delete.len() >= max_deletions {
                break;
            }
            entries_to_delete.push((block_number, location));
            entry = cursor.next()?;
        }
        for (block_number, _location) in &entries_to_delete {
            state_commitment_infos_table.delete(self.txn(), block_number)?;
        }
        Ok((
            self,
            entries_to_delete.last().map(|(block_number, location)| {
                PrunedStateCommitmentInfosPointers {
                    new_lower_bound: block_number.unchecked_next(),
                    data_end_offset: location.next_offset(),
                }
            }),
        ))
    }
}

impl StorageWriter {
    /// Releases the disk blocks of the commitment infos data file below `end`, which no pointer
    /// may point into (see [`PrunedStateCommitmentInfosPointers::data_end_offset`]). Offsets are
    /// unchanged and the range reads as zeros; repeating the call is a no-op.
    pub fn prune_state_commitment_infos_data(&self, end: usize) -> StorageResult<()> {
        self.file_writers.punch_state_commitment_infos_hole_up_to(end)
    }
}
