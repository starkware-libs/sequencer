use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use blockifier::blockifier::stateful_validator::StatefulValidatorError;
use blockifier::execution::errors::ContractClassError;
use blockifier::state::errors::StateError;
use blockifier::transaction::errors::TransactionExecutionError;
use cairo_vm::types::errors::program_errors::ProgramError;
use starknet_api::block::BlockNumber;
use starknet_api::transaction::{Resource, ResourceBounds};
use starknet_api::StarknetApiError;
use thiserror::Error;
use tokio::task::JoinError;

use crate::compiler_version::VersionIdError;

/// Errors directed towards the end-user, as a result of gateway requests.
#[derive(Debug, Error)]
pub enum GatewayError {
    #[error(transparent)]
    CompilationError(#[from] starknet_sierra_compile::compile::CompilationUtilError),
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
}

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
