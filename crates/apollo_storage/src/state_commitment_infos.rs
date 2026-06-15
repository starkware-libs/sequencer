//! Storage for the per-block OS-input commitment infos (state-trie commitment data for the OS).

use starknet_api::block::BlockNumber;
pub use starknet_committer::patricia_merkle_tree::types::StateCommitmentInfos;
use starknet_committer::patricia_merkle_tree::types::StateCommitmentInfosCodecError;

#[cfg(test)]
#[path = "state_commitment_infos_test.rs"]
mod state_commitment_infos_test;

use crate::db::serialization::{StorageSerde, StorageSerdeError};
use crate::db::table_types::Table;
use crate::db::{TransactionKind, RW};
use crate::{OffsetKind, StorageResult, StorageTransaction};

// Encoded with bincode (not serde_json): this is hash/`Felt`-heavy data, where bincode's fixed
// binary encoding is markedly more compact than JSON's hex-string encoding. The trade-off is that
// bincode is positional and not schema-evolution tolerant, which is acceptable here.
impl StorageSerde for StateCommitmentInfos {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
        let compressed = self.compress()?;
        compressed.serialize_into(res)
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        let compressed = Vec::<u8>::deserialize_from(bytes)?;
        Self::decompress(&compressed).ok()
    }
}

impl From<StateCommitmentInfosCodecError> for StorageSerdeError {
    fn from(error: StateCommitmentInfosCodecError) -> Self {
        match error {
            StateCommitmentInfosCodecError::Bincode(error) => StorageSerdeError::Bincode(error),
            StateCommitmentInfosCodecError::Io(error) => StorageSerdeError::Io(error),
        }
    }
}

/// Interface for reading the OS-input commitment infos from storage.
pub trait StateCommitmentInfosStorageReader<Mode: TransactionKind> {
    /// Returns the commitment infos for the given block, or `None` if not stored.
    fn get_state_commitment_infos(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<StateCommitmentInfos>>;
}

/// Interface for writing the OS-input commitment infos to storage.
pub trait StateCommitmentInfosStorageWriter
where
    Self: Sized,
{
    /// Appends the commitment infos for the given block to storage.
    fn append_state_commitment_infos(
        self,
        block_number: BlockNumber,
        state_commitment_infos: &StateCommitmentInfos,
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
    ) -> StorageResult<Option<StateCommitmentInfos>> {
        let table = self.open_table(&self.tables().state_commitment_infos)?;
        let Some(location) = table.get(self.txn(), &block_number)? else {
            return Ok(None);
        };
        Ok(Some(self.file_handlers().get_state_commitment_infos_unchecked(location)?))
    }
}

impl<T: StorageTransaction<Mode = RW>> StateCommitmentInfosStorageWriter for T {
    fn append_state_commitment_infos(
        self,
        block_number: BlockNumber,
        state_commitment_infos: &StateCommitmentInfos,
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
