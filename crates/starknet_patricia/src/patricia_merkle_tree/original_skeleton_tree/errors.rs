use std::fmt::Debug;

use thiserror::Error;

use crate::patricia_merkle_tree::types::NodeIndex;
use crate::storage::errors::{DeserializationError, StorageError};

#[derive(Debug, Error)]
pub enum OriginalSkeletonTreeError {
    #[error(
        "Failed to deserialize the storage value: {0:?} while building the original skeleton tree."
    )]
    Deserialization(#[from] DeserializationError),
    #[error(
        "Unable to read from storage the storage key: {0:?} while building the original skeleton \
         tree."
    )]
    StorageRead(#[from] StorageError),
    #[error("Failed to read the modified leaf at index {0:?}")]
    ReadModificationsError(NodeIndex),
}
