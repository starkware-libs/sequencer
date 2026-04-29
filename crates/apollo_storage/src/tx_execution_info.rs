//! Storage for transaction execution info per block (input to the starknet OS).
//!
//! Import [`TxExecutionInfoStorageReader`] and [`TxExecutionInfoStorageWriter`] to read and write
//! data using a [`StorageTxn`]. These traits are only available when the `os_input` feature is
//! enabled.

use blockifier::transaction::objects::TransactionExecutionInfo;
#[cfg(feature = "os_input")]
use starknet_api::block::BlockNumber;

use crate::compression_utils::{compress, decompress};
use crate::db::serialization::{StorageSerde, StorageSerdeError};
#[cfg(feature = "os_input")]
use crate::db::table_types::Table;
#[cfg(feature = "os_input")]
use crate::db::{TransactionKind, RW};
#[cfg(feature = "os_input")]
use crate::{OffsetKind, StorageResult, StorageTxn};

/// Per-block container of transaction execution infos, stored as a compressed JSON blob.
#[derive(Debug)]
pub(crate) struct TxExecutionInfos(pub Vec<TransactionExecutionInfo>);

impl StorageSerde for TxExecutionInfos {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
        let bytes = serde_json::to_vec(&self.0)?;
        let compressed = compress(bytes.as_slice())?;
        compressed.serialize_into(res)
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        let compressed = Vec::<u8>::deserialize_from(bytes)?;
        let data = decompress(compressed.as_slice()).ok()?;
        serde_json::from_slice(&data).ok().map(TxExecutionInfos)
    }
}

/// Interface for reading transaction execution infos from storage.
#[cfg(feature = "os_input")]
pub trait TxExecutionInfoStorageReader<Mode: TransactionKind> {
    /// Returns the transaction execution infos for the given block, or `None` if not stored.
    fn get_tx_execution_infos(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<Vec<TransactionExecutionInfo>>>;
}

/// Interface for writing transaction execution infos to storage.
#[cfg(feature = "os_input")]
pub trait TxExecutionInfoStorageWriter
where
    Self: Sized,
{
    /// Appends the transaction execution infos for the given block to storage.
    fn append_tx_execution_infos(
        self,
        block_number: BlockNumber,
        tx_execution_infos: Vec<TransactionExecutionInfo>,
    ) -> StorageResult<Self>;

    /// Removes the transaction execution infos for the given block from storage.
    /// If no entry exists for the block, returns without error.
    fn revert_tx_execution_infos(self, block_number: BlockNumber) -> StorageResult<Self>;
}

#[cfg(feature = "os_input")]
impl<Mode: TransactionKind> TxExecutionInfoStorageReader<Mode> for StorageTxn<'_, Mode> {
    fn get_tx_execution_infos(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<Vec<TransactionExecutionInfo>>> {
        let table = self.open_table(&self.tables.tx_execution_infos)?;
        let Some(location) = table.get(&self.txn, &block_number)? else {
            return Ok(None);
        };
        Ok(Some(self.file_handlers.get_tx_execution_infos_unchecked(location)?.0))
    }
}

#[cfg(feature = "os_input")]
impl TxExecutionInfoStorageWriter for StorageTxn<'_, RW> {
    fn append_tx_execution_infos(
        self,
        block_number: BlockNumber,
        tx_execution_infos: Vec<TransactionExecutionInfo>,
    ) -> StorageResult<Self> {
        let file_offset_table = self.txn.open_table(&self.tables.file_offsets)?;
        let tx_execution_infos_table = self.open_table(&self.tables.tx_execution_infos)?;

        let infos = TxExecutionInfos(tx_execution_infos);
        let location = self.file_handlers.append_tx_execution_infos(&infos);
        tx_execution_infos_table.upsert(&self.txn, &block_number, &location)?;
        file_offset_table.upsert(
            &self.txn,
            &OffsetKind::TxExecutionInfo,
            &location.next_offset(),
        )?;

        Ok(self)
    }

    fn revert_tx_execution_infos(self, block_number: BlockNumber) -> StorageResult<Self> {
        let tx_execution_infos_table = self.open_table(&self.tables.tx_execution_infos)?;
        tx_execution_infos_table.delete(&self.txn, &block_number)?;
        Ok(self)
    }
}
