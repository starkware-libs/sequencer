use axum::http::StatusCode;
use blockifier::state::errors::StateError;
use serde_json::{Error as SerdeError, Value};
use starknet_api::block::GasPrice;
use starknet_api::transaction::{Resource, ResourceBounds};
use starknet_gateway_types::errors::GatewaySpecError;
use thiserror::Error;

use crate::compiler_version::{VersionId, VersionIdError};

pub type GatewayResult<T> = Result<T, GatewaySpecError>;

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
