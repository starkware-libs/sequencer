//! Interface for handling the per-block fee market info.
//! Import [`FeeMarketInfoStorageReader`] and [`FeeMarketInfoStorageWriter`] to read and write
//! data related to the fee market using a `StorageTxn`.

use starknet_api::block::{BlockFeeMarketInfo, BlockNumber};

#[cfg(test)]
#[path = "fee_market_test.rs"]
mod fee_market_test;

use crate::db::table_types::Table;
use crate::db::RW;
use crate::{StorageResult, StorageTransaction};

/// Interface for reading the fee market info.
pub trait FeeMarketInfoStorageReader {
    /// Returns the fee market info of the given block number.
    /// Returns `None` if the block number is not found.
    fn get_fee_market_info(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<BlockFeeMarketInfo>>;
}

/// Interface for writing the fee market info.
pub trait FeeMarketInfoStorageWriter
where
    Self: Sized,
{
    /// Inserts the fee market info of the given block number.
    /// An error is returned if the block number already exists.
    fn set_fee_market_info(
        self,
        block_number: BlockNumber,
        fee_market_info: &BlockFeeMarketInfo,
    ) -> StorageResult<Self>;

    /// Reverts the fee market info of the given block number.
    fn revert_fee_market_info(self, block_number: BlockNumber) -> StorageResult<Self>;
}

impl<T: StorageTransaction> FeeMarketInfoStorageReader for T {
    fn get_fee_market_info(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<BlockFeeMarketInfo>> {
        let table = self.open_table(&self.tables().fee_market_infos)?;
        Ok(table.get(self.txn(), &block_number)?)
    }
}

impl<T: StorageTransaction<Mode = RW>> FeeMarketInfoStorageWriter for T {
    fn set_fee_market_info(
        self,
        block_number: BlockNumber,
        fee_market_info: &BlockFeeMarketInfo,
    ) -> StorageResult<Self> {
        let table = self.open_table(&self.tables().fee_market_infos)?;
        table.insert(self.txn(), &block_number, fee_market_info)?;
        Ok(self)
    }

    fn revert_fee_market_info(self, block_number: BlockNumber) -> StorageResult<Self> {
        let table = self.open_table(&self.tables().fee_market_infos)?;
        table.delete(self.txn(), &block_number)?;
        Ok(self)
    }
}
