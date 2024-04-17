use crate::storage::storage_trait::StorageKey;
use derive_more::Display;
use thiserror::Error;

#[derive(Debug, Error)]
#[allow(dead_code)]
pub(crate) enum StorageError {
    #[error("The key {0:?} does not exist in storage.")]
    MissingKey(StorageKey),
}

#[derive(thiserror::Error, Debug, Display)]
#[allow(dead_code)]
pub(crate) enum SerializationError {
    DeserializeError,
    SerializeError,
}
