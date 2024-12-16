use papyrus_storage::StorageError;
use serde::{Deserialize, Serialize};
use starknet_api::core::ContractAddress;
use thiserror::Error;

#[derive(Debug, Error, Serialize, Deserialize, Clone)]
pub enum StateSyncError {
    #[error("Communication error between StateSync and StateSyncRunner")]
    RunnerCommunicationError,
    #[error("Contract address {0} was not found")]
    ContractAddressNotFoundError(ContractAddress),
    // StorageError does not derive Serialize, Deserialize and Clone Traits.
    // We put the string of the error instead.
    #[error("Unexpected storage error: {0}")]
    StorageError(String),
}

impl From<StorageError> for StateSyncError {
    fn from(error: StorageError) -> Self {
        StateSyncError::StorageError(error.to_string())
    }
}
