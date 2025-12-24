use apollo_storage::storage_reader_communication::{StorageReaderRequest, StorageReaderResponse};
use apollo_storage::storage_reader_handler::StorageReaderHandler;
use apollo_storage::storage_reader_server::StorageReaderServerHandler;
use apollo_storage::{StorageError, StorageReader};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockHeaderWithoutHash;
use starknet_api::state::ThinStateDiff;
use starknet_api::transaction::TransactionHash;

use crate::errors::StateSyncError;

pub type StateSyncResult<T> = Result<T, StateSyncError>;

/// A block that came from the state sync.
/// Contains all the data needed to update the state of the system about this block.
///
/// Blocks that came from the state sync are trusted. Therefore, SyncBlock doesn't contain data
/// needed for verifying the block
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SyncBlock {
    pub state_diff: ThinStateDiff,
    // TODO(Matan): decide if we want block hash, parent block hash and full classes here.
    pub account_transaction_hashes: Vec<TransactionHash>,
    pub l1_transaction_hashes: Vec<TransactionHash>,
    pub block_header_without_hash: BlockHeaderWithoutHash,
}

impl SyncBlock {
    pub fn get_all_transaction_hashes(&self) -> Vec<TransactionHash> {
        self.account_transaction_hashes
            .iter()
            .chain(self.l1_transaction_hashes.iter())
            .cloned()
            .collect()
    }
}

pub struct StateSyncStorageReaderServerHandler;

#[async_trait]
impl StorageReaderServerHandler<StorageReaderRequest, StorageReaderResponse>
    for StateSyncStorageReaderServerHandler
{
    async fn handle_request(
        storage_reader: &StorageReader,
        request: StorageReaderRequest,
    ) -> Result<StorageReaderResponse, StorageError> {
        // Validate that the request is relevant to StateSync.
        // StateSync needs state diffs, block headers, and markers for synchronization.
        match &request {
            StorageReaderRequest::GetStateDiffLocation(_)
            | StorageReaderRequest::GetStateDiffFromFile(_)
            | StorageReaderRequest::GetBlockNumberByHash(_)
            | StorageReaderRequest::GetBlockSignatureByNumber(_)
            | StorageReaderRequest::GetMarker(_) => {
                // Request is valid for StateSync, delegate to unified handler
                let handler = StorageReaderHandler::new(storage_reader.clone());
                handler.handle_request(request)
            }
            _ => Err(StorageError::InvalidRequest {
                component: "StateSync".to_string(),
                request_type: format!("{:?}", request),
            }),
        }
    }
}
