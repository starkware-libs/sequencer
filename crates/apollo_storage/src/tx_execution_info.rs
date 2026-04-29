//! Storage for transaction execution info per block (OS input).
//!
//! The data is used as input to the local OS (Starknet OS in Rust).
//!
//! Import [`TxExecutionInfoStorageReader`] and [`TxExecutionInfoStorageWriter`] to read and write
//! data using a [`StorageTxn`]. These traits are only available when the `os_input` feature is
//! enabled.

use shared_execution_objects::central_objects::CentralTransactionExecutionInfo;

use crate::compression_utils::{compress, decompress};
use crate::db::serialization::{StorageSerde, StorageSerdeError};

/// Per-block container of transaction execution infos, stored as a compressed JSON blob.
#[derive(Clone, Debug)]
pub(crate) struct TxExecutionInfos(pub Vec<CentralTransactionExecutionInfo>);

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
