//! Storage for transaction execution info per block (input to the starknet OS).

use blockifier::transaction::objects::TransactionExecutionInfo;

use crate::compression_utils::{compress, decompress};
use crate::db::serialization::{StorageSerde, StorageSerdeError};

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
