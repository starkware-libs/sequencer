use apollo_starknet_client::reader::ReaderClientError;
use apollo_storage::StorageError;
use futures::channel::mpsc::SendError;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, ContractAddress};
use starknet_api::StarknetApiError;
use thiserror::Error;

#[derive(Debug, Error, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum StateSyncError {
    #[error("Communication error between StateSync and StateSyncRunner")]
    RunnerCommunicationError,
    #[error("Block number {0} was not found. State sync might not be synced up to this block.")]
    BlockNotFound(BlockNumber),
    #[error("Contract address {0} was not found")]
    ContractNotFound(ContractAddress),
    #[error("Class hash {0} was not found")]
    ClassNotFound(ClassHash),
    // StorageError and StarknetApiError do not derive Serialize, Deserialize and Clone Traits.
    // We put the string of the errors instead.
    #[error("Unexpected storage error: {0}")]
    StorageError(String),
    // SendError does not derive Serialize and Deserialize Traits.
    // We put the string of the error instead.
    #[error("Error while sending SyncBlock from StateSync to P2pSyncClient")]
    SendError(String),
    #[error("Unexpected starknet api error: {0}")]
    StarknetApiError(String),
    #[error("State is empty, latest block returned None")]
    EmptyState,
    #[error("Error while trying to communicate with feeder gateway: {0}")]
    ReaderClientError(String),
}

impl From<StorageError> for StateSyncError {
    fn from(error: StorageError) -> Self {
        StateSyncError::StorageError(error.to_string())
    }
}

impl From<StarknetApiError> for StateSyncError {
    fn from(error: StarknetApiError) -> Self {
        StateSyncError::StarknetApiError(error.to_string())
    }
}

impl From<SendError> for StateSyncError {
    fn from(error: SendError) -> Self {
        StateSyncError::SendError(error.to_string())
    }
}

impl From<ReaderClientError> for StateSyncError {
    fn from(error: ReaderClientError) -> Self {
        StateSyncError::ReaderClientError(error.to_string())
    }
}
