use std::fmt::Display;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use blockifier::state::errors::StateError;
use enum_assoc::Assoc;
use papyrus_rpc::error::{
    unexpected_error,
    validation_failure,
    JsonRpcError,
    CLASS_ALREADY_DECLARED,
    CLASS_HASH_NOT_FOUND,
    COMPILATION_FAILED,
    COMPILED_CLASS_HASH_MISMATCH,
    CONTRACT_CLASS_SIZE_IS_TOO_LARGE,
    DUPLICATE_TX,
    INSUFFICIENT_ACCOUNT_BALANCE,
    INSUFFICIENT_MAX_FEE,
    INVALID_TRANSACTION_NONCE,
    NON_ACCOUNT,
    UNSUPPORTED_CONTRACT_CLASS_VERSION,
    UNSUPPORTED_TX_VERSION,
};
use serde_json::{Error as SerdeError, Value};
use starknet_api::block::GasPrice;
use starknet_api::transaction::{Resource, ResourceBounds};
use thiserror::Error;

use crate::compiler_version::{VersionId, VersionIdError};

pub type GatewayResult<T> = Result<T, GatewaySpecError>;

impl IntoResponse for GatewaySpecError {
    fn into_response(self) -> Response {
        let as_rpc = self.into_rpc();
        // TODO(Arni): Fix the status code. The status code should be a HTTP status code - not a
        // Json RPC error code. status code.
        let status =
            StatusCode::from_u16(u16::try_from(as_rpc.code).expect("Expecting a valid u16"))
                .expect("Expecting a valid error code");

        let resp = Response::builder()
            .status(status)
            .body((as_rpc.message, as_rpc.data))
            .expect("Expecting valid response");
        let status = resp.status();
        let body = serde_json::to_string(resp.body()).expect("Expecting valid body");
        (status, body).into_response()
    }
}

#[derive(Debug, Error)]
#[cfg_attr(test, derive(PartialEq))]
pub enum StatelessTransactionValidatorError {
    #[error(
        "Calldata length exceeded maximum: length {calldata_length}
        (allowed length: {max_calldata_length})."
    )]
    CalldataTooLong { calldata_length: usize, max_calldata_length: usize },
    #[error(
        "Cannot declare contract class with size of {contract_class_object_size}; max allowed \
         size: {max_contract_class_object_size}."
    )]
    ContractClassObjectSizeTooLarge {
        contract_class_object_size: usize,
        max_contract_class_object_size: usize,
    },
    #[error("Entry points must be unique and sorted.")]
    EntryPointsNotUniquelySorted,
    #[error(transparent)]
    InvalidSierraVersion(#[from] VersionIdError),
    #[error(
        "Signature length exceeded maximum: length {signature_length}
        (allowed length: {max_signature_length})."
    )]
    SignatureTooLong { signature_length: usize, max_signature_length: usize },
    #[error(
        "Sierra versions older than {min_version} or newer than {max_version} are not supported. \
         The Sierra version of the declared contract is {version}."
    )]
    UnsupportedSierraVersion { version: VersionId, min_version: VersionId, max_version: VersionId },
    #[error("Expected a positive amount of {resource:?}. Got {resource_bounds:?}.")]
    ZeroResourceBounds { resource: Resource, resource_bounds: ResourceBounds },
}

impl From<StatelessTransactionValidatorError> for GatewaySpecError {
    fn from(e: StatelessTransactionValidatorError) -> Self {
        match e {
            StatelessTransactionValidatorError::ContractClassObjectSizeTooLarge { .. } => {
                GatewaySpecError::ContractClassSizeIsTooLarge
            }
            StatelessTransactionValidatorError::UnsupportedSierraVersion { .. } => {
                GatewaySpecError::UnsupportedContractClassVersion
            }
            StatelessTransactionValidatorError::CalldataTooLong { .. }
            | StatelessTransactionValidatorError::EntryPointsNotUniquelySorted
            | StatelessTransactionValidatorError::InvalidSierraVersion(..)
            | StatelessTransactionValidatorError::SignatureTooLong { .. }
            | StatelessTransactionValidatorError::ZeroResourceBounds { .. } => {
                GatewaySpecError::ValidationFailure { data: e.to_string() }
            }
        }
    }
}

pub type StatelessTransactionValidatorResult<T> = Result<T, StatelessTransactionValidatorError>;

pub type StatefulTransactionValidatorResult<T> = Result<T, GatewaySpecError>;

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
    #[error("Contract address not found for request {0}")]
    ContractAddressNotFound(Value),
    #[error("Failed to parse gas price {:?}", 0)]
    GasPriceParsingFailure(GasPrice),
    #[error("RPC error: {0}")]
    RPCError(StatusCode),
    #[error(transparent)]
    ReqwestError(#[from] reqwest::Error),
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

/// Error returned by the gateway, adhering to the Starknet RPC error format.
// To get JsonRpcError from GatewaySpecError, use `into_rpc` method.
// TODO(yair): papyrus_rpc has a test that the add_tx functions return the correct error. Make sure
// it is tested when we have a single gateway.
#[derive(Debug, Clone, Eq, PartialEq, Assoc, Error)]
#[func(pub fn into_rpc(self) -> JsonRpcError<String>)]
pub enum GatewaySpecError {
    #[assoc(into_rpc = CLASS_ALREADY_DECLARED)]
    ClassAlreadyDeclared,
    #[assoc(into_rpc = CLASS_HASH_NOT_FOUND)]
    ClassHashNotFound,
    #[assoc(into_rpc = COMPILED_CLASS_HASH_MISMATCH)]
    CompiledClassHashMismatch,
    #[assoc(into_rpc = COMPILATION_FAILED)]
    CompilationFailed,
    #[assoc(into_rpc = CONTRACT_CLASS_SIZE_IS_TOO_LARGE)]
    ContractClassSizeIsTooLarge,
    #[assoc(into_rpc = DUPLICATE_TX)]
    DuplicateTx,
    #[assoc(into_rpc = INSUFFICIENT_ACCOUNT_BALANCE)]
    InsufficientAccountBalance,
    #[assoc(into_rpc = INSUFFICIENT_MAX_FEE)]
    InsufficientMaxFee,
    #[assoc(into_rpc = INVALID_TRANSACTION_NONCE)]
    InvalidTransactionNonce,
    #[assoc(into_rpc = NON_ACCOUNT)]
    NonAccount,
    #[assoc(into_rpc = unexpected_error(_data))]
    UnexpectedError { data: String },
    #[assoc(into_rpc = UNSUPPORTED_CONTRACT_CLASS_VERSION)]
    UnsupportedContractClassVersion,
    #[assoc(into_rpc = UNSUPPORTED_TX_VERSION)]
    UnsupportedTxVersion,
    #[assoc(into_rpc = validation_failure(_data))]
    ValidationFailure { data: String },
}

impl Display for GatewaySpecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let as_rpc = self.clone().into_rpc();
        write!(
            f,
            "{}: {}. data: {}",
            as_rpc.code,
            as_rpc.message,
            serde_json::to_string(&as_rpc.data).unwrap()
        )
    }
}
