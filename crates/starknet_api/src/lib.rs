//! Representations of canonical [`Starknet`] components.
//!
//! [`Starknet`]: https://starknet.io/

pub mod abi;
pub mod block;
pub mod block_hash;
pub mod class_cache;
pub mod compression_utils;
pub mod consensus_transaction;
pub mod contract_class;
pub mod core;
pub mod crypto;
pub mod data_availability;
pub mod deprecated_contract_class;
pub mod executable_transaction;
pub mod execution_resources;
pub mod execution_utils;
pub mod hash;
pub mod rpc_transaction;
pub mod serde_utils;
pub mod staking;
pub mod state;
#[cfg(any(feature = "testing", test))]
pub mod test_utils;
pub mod transaction;
pub mod transaction_hash;
pub mod type_utils;
pub mod versioned_constants_logic;

use std::num::ParseIntError;

use crate::core::{ClassHash, CompiledClassHash};
use crate::transaction::TransactionVersion;

#[derive(Clone, Debug, PartialEq)]
pub struct CasmHashMismatch {
    hash: ClassHash,
    actual: CompiledClassHash,
    expected: CompiledClassHash,
}

impl std::fmt::Display for CasmHashMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Mismatch compiled class hash for class with hash {:#064x}. Actual: {:#064x}, \
             Expected: {:#064x}",
            self.hash.0, self.actual.0, self.expected.0
        )
    }
}

/// The error type returned by StarknetApi.
// Note: if you need `Eq` see InnerDeserializationError's docstring.
#[derive(thiserror::Error, Clone, Debug, PartialEq)]
pub enum StarknetApiError {
    /// An error when a starknet version is out of range.
    #[error("Starknet version {version} is out of range for block hash calculation")]
    BlockHashVersion { version: String },
    #[error("Out of range {string}.")]
    /// An error for when a value is out of range.
    OutOfRange { string: String },
    /// Error when serializing into number.
    #[error(transparent)]
    ParseIntError(#[from] ParseIntError),
    #[error("Failed to convert the resource hex {0:?} to a felt.")]
    ResourceHexToFeltConversion(String),
    /// Missing resource type / duplicated resource type.
    #[error("Missing resource type / duplicated resource type; got {0}.")]
    InvalidResourceMappingInitializer(String),
    #[error("Invalid Starknet version: {0:?}")]
    InvalidStarknetVersion(Vec<u8>),
    #[error("NonzeroGasPrice cannot be zero.")]
    ZeroGasPrice,
    #[error("Gas price conversion error: {0}")]
    GasPriceConversionError(String),
    #[error(
        "Sierra program length must be > 0 for Cairo1, and == 0 for Cairo0. Got: \
         {sierra_program_length:?} for contract class version {contract_class_version:?}"
    )]
    ContractClassVersionSierraProgramLengthMismatch {
        contract_class_version: u8,
        sierra_program_length: usize,
    },
    #[error(
        "Declare transaction version {} must have a contract class of Cairo \
         version {cairo_version:?}.", **declare_version
    )]
    ContractClassVersionMismatch { declare_version: TransactionVersion, cairo_version: u64 },
    #[error("Failed to parse Sierra version: {0}")]
    ParseSierraVersionError(String),
    #[error("Unsupported transaction type: {0}")]
    UnknownTransactionType(String),
    #[error(
        "Mismatch compiled class hash for class with hash {:#064x}. Actual: {:#064x}, Expected: {:#064x}",
        .0.hash.0, .0.actual.0, .0.expected.0
    )]
    DeclareTransactionCasmHashMissMatch(Box<CasmHashMismatch>),
}

pub type StarknetApiResult<T> = Result<T, StarknetApiError>;
