use thiserror::Error;

use crate::storage::errors::{DeserializationError, StorageError};

#[allow(dead_code)]
#[derive(Debug, Error)]
pub(crate) enum OriginalSkeletonTreeError {
    #[error(
        "Failed to deserialize the storage value: {0:?} while building the original skeleton tree."
    )]
    Deserialization(#[from] DeserializationError),
    #[error(
        "Unable to read from storage the storage key: {0:?} while building the \
         original skeleton tree."
    )]
    StorageRead(#[from] StorageError),
}
