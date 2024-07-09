//! Representations of canonical [`Starknet`] components.
//!
//! [`Starknet`]: https://starknet.io/

pub mod block;
pub mod block_hash;
pub mod core;
pub mod crypto;
pub mod data_availability;
pub mod deprecated_contract_class;
pub mod hash;
pub mod internal_transaction;
pub mod rpc_transaction;
pub mod serde_utils;
pub mod state;
pub mod transaction;
pub mod transaction_hash;
pub mod type_utils;

use std::num::ParseIntError;

use serde_utils::InnerDeserializationError;

/// The error type returned by StarknetApi.
#[derive(thiserror::Error, Clone, Debug)]
pub enum StarknetApiError {
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
}
