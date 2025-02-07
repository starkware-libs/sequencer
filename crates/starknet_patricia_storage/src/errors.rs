use serde_json;
use starknet_types_core::felt::FromStrError;
use thiserror::Error;

use crate::storage::storage_trait::StorageKey;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("The key {0:?} does not exist in storage.")]
    MissingKey(StorageKey),
}

#[derive(thiserror::Error, Debug)]
pub enum SerializationError {
    #[error("Serialize error: {0}")]
    SerializeError(#[from] serde_json::Error),
}

#[derive(thiserror::Error, Debug)]
pub enum DeserializationError {
    #[error("There is a key duplicate at {0} mapping.")]
    KeyDuplicate(String),
    #[error("The key {0} unexpectedly doesn't exist.")]
    NonExistingKey(String),
    #[error(transparent)]
    ParsingError(#[from] serde_json::Error),
    #[error("Unexpected prefix ({0:?}) variant when deserializing a leaf.")]
    // TODO(Aviv, 17/07/2024): Define a trait `T` for storage prefix and return `impl T` here.
    LeafPrefixError(Vec<u8>),
    #[error(transparent)]
    StringConversionError(#[from] std::str::Utf8Error),
    #[error(transparent)]
    FeltParsingError(#[from] FromStrError),
    #[error("Encountered an invalid type when deserializing a leaf.")]
    LeafTypeError,
    #[error("Invalid value for deserialization: {0}.")]
    ValueError(Box<dyn std::error::Error>),
}
