use serde_json;
use starknet_api::StarknetApiError;
use starknet_types_core::felt::FromStrError;
use thiserror::Error;

use crate::storage_trait::{DbKey, DbValue};

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("The key {0:?} does not exist in storage.")]
    MissingKey(DbKey),
}

pub type SerializationResult<T> = Result<T, SerializationError>;

#[derive(thiserror::Error, Debug)]
pub enum SerializationError {
    #[error(transparent)]
    IOSerialize(#[from] std::io::Error),
    #[error("Failed to serialize to JSON: {0}")]
    JsonSerializeError(#[from] serde_json::Error),
}

#[derive(thiserror::Error, Debug)]
pub enum DeserializationError {
    #[error("Failed to deserialize raw felt: {0:?}")]
    FeltDeserialization(DbValue),
    #[error(transparent)]
    FeltParsingError(#[from] FromStrError),
    #[error("There is a key duplicate at {0} mapping.")]
    KeyDuplicate(String),
    #[error("Unexpected prefix ({0:?}) variant when deserializing a leaf.")]
    // TODO(Aviv, 17/07/2024): Define a trait `T` for storage prefix and return `impl T` here.
    LeafPrefixError(Vec<u8>),
    #[error("Encountered an invalid type when deserializing a leaf.")]
    LeafTypeError,
    #[error("The key {0} unexpectedly doesn't exist.")]
    NonExistingKey(String),
    #[error(transparent)]
    ParsingError(#[from] serde_json::Error),
    #[error(transparent)]
    StarknetApiError(#[from] StarknetApiError),
    #[error(transparent)]
    StringConversionError(#[from] std::str::Utf8Error),
    // TODO(Ariel): This is only used for EdgeNode construction failures (path length etc.), add
    // error types here and use them instead of the general ValueError.
    #[error("Invalid value for deserialization: {0}.")]
    ValueError(Box<dyn std::error::Error>),
}
