use futures::channel::mpsc::SendError;
use papyrus_storage::StorageError;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error, Serialize, Deserialize, Clone)]
pub enum StateSyncError {
    #[error("Communication error between StateSync and StateSyncRunner")]
    RunnerCommunicationError,
    // StorageError does not derive Serialize, Deserialize and Clone Traits.
    // We put the string of the error instead.
    #[error("Unexpected storage error: {0}")]
    StorageError(String),
    // SendError does not derive Serialize and Deserialize Traits.
    // We put the string of the error instead.
    #[error("Error while sending SyncBlock from StateSync to P2pSyncClient")]
    SendError(String),
}

impl From<StorageError> for StateSyncError {
    fn from(error: StorageError) -> Self {
        StateSyncError::StorageError(error.to_string())
    }
}

impl From<SendError> for StateSyncError {
    fn from(error: SendError) -> Self {
        StateSyncError::SendError(error.to_string())
    }
}
