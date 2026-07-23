//! Storage for the set of state-trie leaves accessed per block (input to the starknet OS).

pub use blockifier::state::accessed_keys::AccessedKeys;
use starknet_api::block::BlockNumber;

use crate::compression_utils::{compress, decompress};
use crate::db::serialization::{StorageSerde, StorageSerdeError};
use crate::db::table_types::Table;
use crate::db::{TransactionKind, RW};
use crate::{OffsetKind, StorageResult, StorageTransaction};

impl StorageSerde for AccessedKeys {
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

/// Interface for reading the accessed-key set from storage.
pub trait AccessedKeysStorageReader<Mode: TransactionKind> {
    /// Returns the accessed-key set for the given block, or `None` if not stored.
    fn get_accessed_keys(&self, block_number: BlockNumber) -> StorageResult<Option<AccessedKeys>>;
}

/// Interface for writing the accessed-key set to storage.
pub trait AccessedKeysStorageWriter
where
    Self: Sized,
{
    /// Appends the accessed-key set for the given block to storage.
    fn append_accessed_keys(
        self,
        block_number: BlockNumber,
        accessed_keys: &AccessedKeys,
    ) -> StorageResult<Self>;

    /// Removes the accessed-key set for the given block from storage.
    /// If no entry exists for the block, returns without error.
    fn revert_accessed_keys(self, block_number: BlockNumber) -> StorageResult<Self>;
}

impl<T: StorageTransaction> AccessedKeysStorageReader<<T as StorageTransaction>::Mode> for T {
    fn get_accessed_keys(&self, block_number: BlockNumber) -> StorageResult<Option<AccessedKeys>> {
        let table = self.open_table(&self.tables().accessed_keys)?;
        let Some(location) = table.get(self.txn(), &block_number)? else {
            return Ok(None);
        };
        Ok(Some(self.file_handlers().get_accessed_keys_unchecked(location)?))
    }
}

impl<T: StorageTransaction<Mode = RW>> AccessedKeysStorageWriter for T {
    fn append_accessed_keys(
        self,
        block_number: BlockNumber,
        accessed_keys: &AccessedKeys,
    ) -> StorageResult<Self> {
        let file_offset_table = self.open_table(&self.tables().file_offsets)?;
        let accessed_keys_table = self.open_table(&self.tables().accessed_keys)?;

        let location = self.file_handlers().append_accessed_keys(accessed_keys);
        accessed_keys_table.upsert(self.txn(), &block_number, &location)?;
        file_offset_table.upsert(self.txn(), &OffsetKind::AccessedKeys, &location.next_offset())?;

        Ok(self)
    }

    fn revert_accessed_keys(self, block_number: BlockNumber) -> StorageResult<Self> {
        let accessed_keys_table = self.open_table(&self.tables().accessed_keys)?;
        accessed_keys_table.delete(self.txn(), &block_number)?;
        Ok(self)
    }
}
