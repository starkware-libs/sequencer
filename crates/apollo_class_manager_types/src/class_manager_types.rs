use apollo_storage::storage_reader_server::StorageReaderServerHandler;
use apollo_storage::{StorageError, StorageReader};
use async_trait::async_trait;
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

pub struct ClassManagerStorageReaderServerHandler;

#[async_trait]
impl StorageReaderServerHandler<ClassManagerStorageRequest, ClassManagerStorageResponse>
    for ClassManagerStorageReaderServerHandler
{
    async fn handle_request(
        _storage_reader: &StorageReader,
        _request: ClassManagerStorageRequest,
    ) -> Result<ClassManagerStorageResponse, StorageError> {
        // TODO(Dean/Nadin): Implement the logic for the class manager storage reader server
        // handler.
        unimplemented!()
    }
}
