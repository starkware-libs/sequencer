use thiserror::Error;

use crate::storage::errors::StorageError;
use crate::storage::storage_trait::StorageValue;

// TODO(Amos, 01/04/2024): Add error types.
#[allow(dead_code)]
#[derive(Debug, Error)]
pub(crate) enum OriginalSkeletonTreeError {
    #[error(
        "Failed to deserialize the storage value: {0:?} while building the original skeleton tree."
    )]
    Deserialization(StorageValue),
    #[error(
        "Unable to read from storage the storage key: {0:?} while building the \
         original skeleton tree."
    )]
    StorageRead(#[from] StorageError),
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum UpdatedSkeletonTreeError {
    MissingNode,
    PoisonedLock(String),
    NonDroppedPointer(String),
}

#[derive(thiserror::Error, Debug, derive_more::Display)]
pub(crate) enum FilledTreeError {
    MissingRoot,
    SerializeError(#[from] serde_json::Error),
}
