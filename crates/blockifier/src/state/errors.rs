use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_vm::types::errors::program_errors::ProgramError;
use num_bigint::{BigUint, TryFromBigIntError};
use starknet_api::core::{ClassHash, ContractAddress};
use starknet_api::state::SierraContractClass;
use starknet_api::StarknetApiError;
use thiserror::Error;

use crate::abi::constants;

#[derive(Debug, Error)]
pub enum StateError {
    #[error("CASM and Sierra mismatch for class hash {:#064x}: {message}.", class_hash.0)]
    CasmAndSierraMismatch { class_hash: ClassHash, message: String },
    #[error(transparent)]
    FromBigUint(#[from] TryFromBigIntError<BigUint>),
    #[error(
        "A block hash must be provided for block number > {}.",
        constants::STORED_BLOCK_HASH_BUFFER
    )]
    OldBlockHashNotProvided,
    #[error("Cannot deploy contract at address 0.")]
    OutOfRangeContractAddress,
    #[error(transparent)]
    ProgramError(#[from] ProgramError),
    #[error("Deployment failed: contract already deployed at address {:#064x}", ***.0)]
    UnavailableContractAddress(ContractAddress),
    #[error("Class with hash {:#064x} is not declared.", **.0)]
    UndeclaredClassHash(ClassHash),
    #[error(transparent)]
    StarknetApiError(#[from] StarknetApiError),
    /// Represents all unexpected errors that may occur while reading from state.
    #[error("Failed to read from state: {0}.")]
    StateReadError(String),
}

/// Converts CASM and Sierra options into a Result, raising a `CasmAndSierraMismatch` error
/// when there is an inconsistency.
pub fn validate_casm_and_sierra(
    class_hash: ClassHash,
    casm_opt: Option<CasmContractClass>,
    sierra_opt: Option<SierraContractClass>,
) -> Result<Option<(CasmContractClass, SierraContractClass)>, StateError> {
    match (casm_opt, sierra_opt) {
        (Some(casm), Some(sierra)) => Ok(Some((casm, sierra))),
        (Some(_), None) => Err(StateError::CasmAndSierraMismatch {
            class_hash,
            message: "Class exists in CASM but not in Sierra".to_string(),
        }),
        (None, Some(_)) => Err(StateError::CasmAndSierraMismatch {
            class_hash,
            message: "Class exists in Sierra but not in CASM".to_string(),
        }),
        (None, None) => Ok(None),
    }
}
