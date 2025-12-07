use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHeader, BlockNumber};

// TODO(Dean): Fill in with actual storage table names and operations.
/// Storage-related requests for the class manager.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum ClassManagerStorageRequest {
    /// Request to read data in Table1 for the given block height.
    Table1Replacer(BlockNumber),
}

// TODO(Dean): Fill in with actual response types matching the request variants.
/// Response for class manager storage requests.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum ClassManagerStorageResponse {
    /// Table1 data for the requested operation.
    Table1Replacer(BlockHeader),
}
