use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHeader, BlockNumber};

// TODO(Nadin/Dean): Fill in with actual storage table names and operations.
/// Storage-related requests.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum StorageReaderRequest {
    /// Request to read data in Table1 for the given block height.
    Table1Replacer(BlockNumber),
}

// TODO(Nadin/Dean): Fill in with actual response types matching the request variants.
/// Storage-related response.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum StorageReaderResponse {
    /// Table1 data for the requested operation.
    Table1Replacer(BlockHeader),
}
