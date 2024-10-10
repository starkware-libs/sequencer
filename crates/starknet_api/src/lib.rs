//! Representations of canonical [`Starknet`] components.
//!
//! [`Starknet`]: https://starknet.io/

pub mod block;
pub mod block_hash;
pub mod contract_class;
pub mod core;
pub mod crypto;
pub mod data_availability;
pub mod deprecated_contract_class;
pub mod executable_transaction;
pub mod execution_resources;
pub mod hash;
pub mod rpc_transaction;
pub mod serde_utils;
pub mod state;
#[cfg(any(feature = "testing", test))]
pub mod test_utils;
pub mod transaction;
pub mod transaction_hash;
pub mod type_utils;

use std::num::ParseIntError;

use serde_utils::InnerDeserializationError;

/// The error type returned by StarknetApi.
// Note: if you need `Eq` see InnerDeserializationError's docstring.
#[derive(thiserror::Error, Clone, Debug, PartialEq)]
pub enum StarknetApiError {
    /// An error when a starknet version is out of range.
    #[error("Starknet version {version} is out of range for block hash calculation")]
    BlockHashVersion { version: String },
    /// Error in the inner deserialization of the node.
    #[error(transparent)]
    InnerDeserialization(#[from] InnerDeserializationError),
    #[error("Out of range {string}.")]
    /// An error for when a value is out of range.
    OutOfRange { string: String },
    /// Error when serializing into number.
    #[error(transparent)]
    ParseIntError(#[from] ParseIntError),
    /// Missing resource type / duplicated resource type.
    #[error("Missing resource type / duplicated resource type; got {0}.")]
    InvalidResourceMappingInitializer(String),
    #[error("NonzeroGasPrice cannot be zero.")]
    ZeroGasPrice,
}

pub type StarknetApiResult<T> = Result<T, StarknetApiError>;
