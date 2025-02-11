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
}

/// Interface for writing data related to the class manager.
pub trait ClassManagerStorageWriter
where
    Self: Sized,
{
    /// Updates the block marker of the class manager.
    // To enforce that no commit happen after a failure, we consume and return Self on success.
    fn update_class_manager_block_marker(self, block_number: &BlockNumber) -> StorageResult<Self>;

    /// When reverting a block, if the class manager marker points to the block afterward, revert
    /// the marker.
    fn try_revert_class_manager_marker(
        self,
        reverted_block_number: BlockNumber,
    ) -> StorageResult<Self>;
}

impl<Mode: TransactionKind> ClassManagerStorageReader for StorageTxn<'_, Mode> {
    fn get_class_manager_block_marker(&self) -> StorageResult<BlockNumber> {
        let markers_table = self.open_table(&self.tables.markers)?;
        Ok(markers_table.get(&self.txn, &MarkerKind::ClassManagerBlock)?.unwrap_or_default())
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
        reverted_block_number: BlockNumber,
    ) -> StorageResult<Self> {
        let cur_marker = self.get_class_manager_block_marker()?;
        if cur_marker == reverted_block_number.unchecked_next() {
            Ok(self.update_class_manager_block_marker(&reverted_block_number)?)
        } else {
            Ok(self)
        }
    }
}
