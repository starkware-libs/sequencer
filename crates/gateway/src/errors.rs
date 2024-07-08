use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use blockifier::blockifier::stateful_validator::StatefulValidatorError;
use blockifier::execution::errors::ContractClassError;
use blockifier::state::errors::StateError;
use blockifier::transaction::errors::TransactionExecutionError;
use cairo_vm::types::errors::program_errors::ProgramError;
use serde_json::{Error as SerdeError, Value};
use starknet_api::block::{BlockNumber, GasPrice};
use starknet_api::core::CompiledClassHash;
use starknet_api::transaction::{Resource, ResourceBounds};
use starknet_api::StarknetApiError;
use starknet_sierra_compile::errors::CompilationUtilError;
use thiserror::Error;
use tokio::task::JoinError;

use crate::compiler_version::{VersionId, VersionIdError};

/// Errors directed towards the end-user, as a result of gateway requests.
#[derive(Debug, Error)]
pub enum GatewayError {
    #[error(transparent)]
    CompilationError(#[from] CompilationUtilError),
    #[error(
        "The supplied compiled class hash {supplied:?} does not match the hash of the Casm class \
         compiled from the supplied Sierra {hash_result:?}"
    )]
    CompiledClassHashMismatch { supplied: CompiledClassHash, hash_result: CompiledClassHash },
    #[error(transparent)]
    DeclaredContractClassError(#[from] ContractClassError),
    #[error(transparent)]
    DeclaredContractProgramError(#[from] ProgramError),
    #[error("Internal server error: {0}")]
    InternalServerError(#[from] JoinError),
    #[error("Error sending message: {0}")]
    MessageSendError(String),
    #[error(transparent)]
    StatefulTransactionValidatorError(#[from] StatefulTransactionValidatorError),
    #[error(transparent)]
    StatelessTransactionValidatorError(#[from] StatelessTransactionValidatorError),
    #[error("{builtins:?} is not a subsquence of {supported_builtins:?}")]
    UnsupportedBuiltins { builtins: Vec<String>, supported_builtins: Vec<String> },
}

pub type GatewayResult<T> = Result<T, GatewayError>;

impl IntoResponse for GatewayError {
    // TODO(Arni, 1/5/2024): Be more fine tuned about the error response. Not all Gateway errors
    // are internal server errors.
    fn into_response(self) -> Response {
        let body = self.to_string();
        (StatusCode::INTERNAL_SERVER_ERROR, body).into_response()
    }
}

#[derive(Debug, Error)]
#[cfg_attr(test, derive(PartialEq))]
pub enum StatelessTransactionValidatorError {
    #[error("Expected a positive amount of {resource:?}. Got {resource_bounds:?}.")]
    ZeroResourceBounds { resource: Resource, resource_bounds: ResourceBounds },
    #[error(
        "Calldata length exceeded maximum: length {calldata_length}
        (allowed length: {max_calldata_length})."
    )]
    CalldataTooLong { calldata_length: usize, max_calldata_length: usize },
    #[error(
        "Signature length exceeded maximum: length {signature_length}
        (allowed length: {max_signature_length})."
    )]
    SignatureTooLong { signature_length: usize, max_signature_length: usize },
    #[error(transparent)]
    InvalidSierraVersion(#[from] VersionIdError),
    #[error(
        "Sierra versions older than {min_version} or newer than {max_version} are not supported. \
         The Sierra version of the declared contract is {version}."
    )]
    UnsupportedSierraVersion { version: VersionId, min_version: VersionId, max_version: VersionId },
    #[error(
        "Cannot declare contract class with bytecode size of {bytecode_size}; max allowed size: \
         {max_bytecode_size}."
    )]
    BytecodeSizeTooLarge { bytecode_size: usize, max_bytecode_size: usize },
    #[error(
        "Cannot declare contract class with size of {contract_class_object_size}; max allowed \
         size: {max_contract_class_object_size}."
    )]
    ContractClassObjectSizeTooLarge {
        contract_class_object_size: usize,
        max_contract_class_object_size: usize,
    },
}

pub type StatelessTransactionValidatorResult<T> = Result<T, StatelessTransactionValidatorError>;

#[derive(Debug, Error)]
pub enum StatefulTransactionValidatorError {
    #[error("Block number {block_number:?} is out of range.")]
    OutOfRangeBlockNumber { block_number: BlockNumber },
    #[error(transparent)]
    StarknetApiError(#[from] StarknetApiError),
    #[error(transparent)]
    StateError(#[from] StateError),
    #[error(transparent)]
    StatefulValidatorError(#[from] StatefulValidatorError),
    #[error(transparent)]
    TransactionExecutionError(#[from] TransactionExecutionError),
}

pub type StatefulTransactionValidatorResult<T> = Result<T, StatefulTransactionValidatorError>;

/// Errors originating from `[`Gateway::run`]` command, to be handled by infrastructure code.
#[derive(Debug, Error)]
pub enum GatewayRunError {
    #[error(transparent)]
    ServerStartupError(#[from] hyper::Error),
}

#[derive(Debug, Error)]
pub enum RPCStateReaderError {
    #[error("Block not found for request {0}")]
    BlockNotFound(Value),
    #[error("Class hash not found for request {0}")]
    ClassHashNotFound(Value),
    #[error("Failed to parse gas price {:?}", 0)]
    GasPriceParsingFailure(GasPrice),
    #[error("Contract address not found for request {0}")]
    ContractAddressNotFound(Value),
    #[error(transparent)]
    ReqwestError(#[from] reqwest::Error),
    #[error("RPC error: {0}")]
    RPCError(StatusCode),
    #[error("Unexpected error code: {0}")]
    UnexpectedErrorCode(u16),
}

pub type RPCStateReaderResult<T> = Result<T, RPCStateReaderError>;

impl From<RPCStateReaderError> for StateError {
    fn from(err: RPCStateReaderError) -> Self {
        match err {
            RPCStateReaderError::ClassHashNotFound(request) => {
                match serde_json::from_value(request["params"]["class_hash"].clone()) {
                    Ok(class_hash) => StateError::UndeclaredClassHash(class_hash),
                    Err(e) => serde_err_to_state_err(e),
                }
            }
            _ => StateError::StateReadError(err.to_string()),
        }
    }
}

// Converts a serde error to the error type of the state reader.
pub fn serde_err_to_state_err(err: SerdeError) -> StateError {
    StateError::StateReadError(format!("Failed to parse rpc result {:?}", err.to_string()))
}
