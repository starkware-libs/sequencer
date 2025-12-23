use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::state::ThinStateDiff;

use crate::mmap_file::LocationInFile;
use crate::storage_reader_server::{StorageReaderServer, StorageReaderServerHandler};
use crate::{StorageError, StorageReader};

/// Type alias for the generic storage reader server.
pub type GenericStorageReaderServer = StorageReaderServer<
    GenericStorageReaderServerHandler,
    StorageReaderRequest,
    StorageReaderResponse,
>;

// TODO(Nadin/Dean): Fill in with actual storage table names and operations.
/// Storage-related requests.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum StorageReaderRequest {
    /// Request to get the location of a state diff for a given block number.
    StateDiffLocation(BlockNumber),
    /// Request to get a thin state diff from a specific file location.
    ThinStateDiff(LocationInFile),
}

// TODO(Nadin/Dean): Fill in with actual response types matching the request variants.
/// Storage-related response.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum StorageReaderResponse {
    /// Response containing the file location of a state diff.
    StateDiffLocation(LocationInFile),
    /// Response containing the thin state diff data.
    ThinStateDiff(ThinStateDiff),
}

/// Generic handler for storage reader requests.
pub struct GenericStorageReaderServerHandler;

#[async_trait]
impl StorageReaderServerHandler<StorageReaderRequest, StorageReaderResponse>
    for GenericStorageReaderServerHandler
{
    async fn handle_request(
        storage_reader: &StorageReader,
        request: StorageReaderRequest,
    ) -> Result<StorageReaderResponse, StorageError> {
        let txn = storage_reader.begin_ro_txn()?;
        match request {
            StorageReaderRequest::StateDiffLocation(block_number) => {
                let state_diff_location =
                    txn.get_state_diff_location(block_number)?.ok_or(StorageError::NotFound {
                        resource_type: "State diff".to_string(),
                        resource_id: block_number.to_string(),
                    })?;
                Ok(StorageReaderResponse::StateDiffLocation(state_diff_location))
            }
            StorageReaderRequest::ThinStateDiff(location) => {
                let state_diff = txn.get_state_diff_from_location(location)?;
                Ok(StorageReaderResponse::ThinStateDiff(state_diff))
            }
        }
    }
}
