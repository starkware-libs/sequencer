//! Interface for handling data related to the class manager.
// TODO(noamsp): Add Documentation
#[cfg(test)]
#[path = "class_manager_test.rs"]
mod class_manager_test;

use starknet_api::block::BlockNumber;

use crate::db::table_types::Table;
use crate::db::{TransactionKind, RW};
use crate::{MarkerKind, StorageResult, StorageTxn};

/// Interface for reading data related to the class manager.
pub trait ClassManagerStorageReader {
    /// The block number marker is the first block number that doesn't exist yet in the class
    /// manager.
    fn get_class_manager_block_marker(&self) -> StorageResult<BlockNumber>;

    /// The block number marker is the first block number that the class manager supports
    /// compilation from.
    fn get_compiler_backward_compatibility_marker(&self) -> StorageResult<BlockNumber>;
}

/// Interface for writing data related to the class manager.
pub trait ClassManagerStorageWriter
where
    Self: Sized,
{
    /// Updates the block marker of the class manager.
    // To enforce that no commit happen after a failure, we consume and return Self on success.
    fn update_class_manager_block_marker(self, block_number: &BlockNumber) -> StorageResult<Self>;

    /// Reverts the class manager marker by one block if the current marker in storage is
    /// `target_block_number + 1`. This means it can only revert one height at a time.
    fn try_revert_class_manager_marker(
        self,
        target_block_number: BlockNumber,
    ) -> StorageResult<Self>;

    /// Updates the block marker of compiler backward compatibility, marking the blocks behind the
    /// given number as non-backward-compatible.
    // To enforce that no commit happen after a failure, we consume and return Self on success.
    fn update_compiler_backward_compatibility_marker(
        self,
        block_number: &BlockNumber,
    ) -> StorageResult<Self>;
}

impl<Mode: TransactionKind> ClassManagerStorageReader for StorageTxn<'_, Mode> {
    fn get_class_manager_block_marker(&self) -> StorageResult<BlockNumber> {
        let markers_table = self.open_table(&self.tables.markers)?;
        Ok(markers_table.get(&self.txn, &MarkerKind::ClassManagerBlock)?.unwrap_or_default())
    }

    fn get_compiler_backward_compatibility_marker(&self) -> StorageResult<BlockNumber> {
        let markers_table = self.open_table(&self.tables.markers)?;
        Ok(markers_table
            .get(&self.txn, &MarkerKind::CompilerBackwardCompatibility)?
            .unwrap_or_default())
    }
}

impl ClassManagerStorageWriter for StorageTxn<'_, RW> {
    fn update_class_manager_block_marker(self, block_number: &BlockNumber) -> StorageResult<Self> {
        let markers_table = self.open_table(&self.tables.markers)?;
        markers_table.upsert(&self.txn, &MarkerKind::ClassManagerBlock, block_number)?;
        Ok(self)
    }

    fn try_revert_class_manager_marker(
        self,
        target_block_number: BlockNumber,
    ) -> StorageResult<Self> {
        let cur_marker = self.get_class_manager_block_marker()?;
        if cur_marker == target_block_number.unchecked_next() {
            Ok(self.update_class_manager_block_marker(&target_block_number)?)
        } else {
            Ok(self)
        }
    }

    fn update_compiler_backward_compatibility_marker(
        self,
        block_number: &BlockNumber,
    ) -> StorageResult<Self> {
        let markers_table = self.open_table(&self.tables.markers)?;
        markers_table.upsert(
            &self.txn,
            &MarkerKind::CompilerBackwardCompatibility,
            block_number,
        )?;
        Ok(self)
    }
}
