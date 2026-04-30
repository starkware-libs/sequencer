//! Storage for state commitment infos per block (input to the starknet OS).

use starknet_api::block::BlockNumber;
use starknet_os::commitment_infos::StateCommitmentInfos;

use crate::compression_utils::{compress, decompress};
use crate::db::serialization::{StorageSerde, StorageSerdeError};
use crate::db::table_types::Table;
use crate::db::{TransactionKind, RW};
use crate::{OffsetKind, StorageResult, StorageTxn};

/// Per-block state commitment infos, stored as a compressed JSON blob.
#[derive(Debug)]
pub(crate) struct StateCommitmentInfosForStorage(pub StateCommitmentInfos);

impl StorageSerde for StateCommitmentInfosForStorage {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
        let bytes = serde_json::to_vec(&self.0)?;
        let compressed = compress(bytes.as_slice())?;
        compressed.serialize_into(res)
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        let compressed = Vec::<u8>::deserialize_from(bytes)?;
        let data = decompress(compressed.as_slice()).ok()?;
        serde_json::from_slice(&data).ok().map(StateCommitmentInfosForStorage)
    }
}

/// Interface for reading state commitment infos from storage.
pub trait StateCommitmentInfosStorageReader<Mode: TransactionKind> {
    /// Returns the state commitment infos for the given block, or `None` if not stored.
    fn get_state_commitment_infos(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<StateCommitmentInfos>>;
}

/// Interface for writing state commitment infos to storage.
pub trait StateCommitmentInfosStorageWriter
where
    Self: Sized,
{
    /// Appends the state commitment infos for the given block to storage.
    fn append_state_commitment_infos(
        self,
        block_number: BlockNumber,
        state_commitment_infos: StateCommitmentInfos,
    ) -> StorageResult<Self>;

    /// Removes the state commitment infos for the given block from storage.
    /// If no entry exists for the block, returns without error.
    fn revert_state_commitment_infos(self, block_number: BlockNumber) -> StorageResult<Self>;
}

impl<Mode: TransactionKind> StateCommitmentInfosStorageReader<Mode> for StorageTxn<'_, Mode> {
    fn get_state_commitment_infos(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<StateCommitmentInfos>> {
        let table = self.open_table(&self.tables.state_commitment_infos)?;
        let Some(location) = table.get(&self.txn, &block_number)? else {
            return Ok(None);
        };
        Ok(Some(self.file_handlers.get_state_commitment_infos_unchecked(location)?.0))
    }
}

impl StateCommitmentInfosStorageWriter for StorageTxn<'_, RW> {
    fn append_state_commitment_infos(
        self,
        block_number: BlockNumber,
        state_commitment_infos: StateCommitmentInfos,
    ) -> StorageResult<Self> {
        let file_offset_table = self.txn.open_table(&self.tables.file_offsets)?;
        let state_commitment_infos_table = self.open_table(&self.tables.state_commitment_infos)?;

        let infos = StateCommitmentInfosForStorage(state_commitment_infos);
        let location = self.file_handlers.append_state_commitment_infos(&infos);
        state_commitment_infos_table.upsert(&self.txn, &block_number, &location)?;
        file_offset_table.upsert(
            &self.txn,
            &OffsetKind::StateCommitmentInfos,
            &location.next_offset(),
        )?;

        Ok(self)
    }

    fn revert_state_commitment_infos(self, block_number: BlockNumber) -> StorageResult<Self> {
        let state_commitment_infos_table = self.open_table(&self.tables.state_commitment_infos)?;
        state_commitment_infos_table.delete(&self.txn, &block_number)?;
        Ok(self)
    }
}
