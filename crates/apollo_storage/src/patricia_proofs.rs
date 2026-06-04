//! Storage for the merged Patricia witness proofs per block (input to the starknet OS).

#[cfg(test)]
#[path = "patricia_proofs_test.rs"]
mod patricia_proofs_test;

use starknet_api::block::BlockNumber;
pub use starknet_committer::patricia_merkle_tree::types::{
    ContractsTrieProof,
    StarknetForestProofs,
};

use crate::compression_utils::{compress, decompress};
use crate::db::serialization::{StorageSerde, StorageSerdeError};
use crate::db::table_types::Table;
use crate::db::{TransactionKind, RW};
use crate::{OffsetKind, StorageResult, StorageTxn};

impl StorageSerde for StarknetForestProofs {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
        let bytes = serde_json::to_vec(self)?;
        let compressed = compress(bytes.as_slice())?;
        compressed.serialize_into(res)
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        let compressed = Vec::<u8>::deserialize_from(bytes)?;
        let data = decompress(compressed.as_slice()).ok()?;
        serde_json::from_slice(&data).ok()
    }
}

/// Interface for reading the Patricia witness proofs from storage.
pub trait PatriciaProofsStorageReader<Mode: TransactionKind> {
    /// Returns the Patricia witness proofs for the given block, or `None` if not stored.
    fn get_patricia_proofs(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<StarknetForestProofs>>;
}

/// Interface for writing the Patricia witness proofs to storage.
pub trait PatriciaProofsStorageWriter
where
    Self: Sized,
{
    /// Appends the Patricia witness proofs for the given block to storage.
    fn append_patricia_proofs(
        self,
        block_number: BlockNumber,
        patricia_proofs: &StarknetForestProofs,
    ) -> StorageResult<Self>;

    /// Removes the Patricia witness proofs for the given block from storage.
    /// If no entry exists for the block, returns without error.
    fn revert_patricia_proofs(self, block_number: BlockNumber) -> StorageResult<Self>;
}

impl<Mode: TransactionKind> PatriciaProofsStorageReader<Mode> for StorageTxn<'_, Mode> {
    fn get_patricia_proofs(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<StarknetForestProofs>> {
        let table = self.open_table(&self.tables.patricia_proofs)?;
        let Some(location) = table.get(&self.txn, &block_number)? else {
            return Ok(None);
        };
        Ok(Some(self.file_handlers.get_patricia_proofs_unchecked(location)?))
    }
}

impl PatriciaProofsStorageWriter for StorageTxn<'_, RW> {
    fn append_patricia_proofs(
        self,
        block_number: BlockNumber,
        patricia_proofs: &StarknetForestProofs,
    ) -> StorageResult<Self> {
        let file_offset_table = self.txn.open_table(&self.tables.file_offsets)?;
        let patricia_proofs_table = self.open_table(&self.tables.patricia_proofs)?;

        let location = self.file_handlers.append_patricia_proofs(patricia_proofs);
        patricia_proofs_table.upsert(&self.txn, &block_number, &location)?;
        file_offset_table.upsert(
            &self.txn,
            &OffsetKind::PatriciaProofs,
            &location.next_offset(),
        )?;

        Ok(self)
    }

    fn revert_patricia_proofs(self, block_number: BlockNumber) -> StorageResult<Self> {
        let patricia_proofs_table = self.open_table(&self.tables.patricia_proofs)?;
        patricia_proofs_table.delete(&self.txn, &block_number)?;
        Ok(self)
    }
}
