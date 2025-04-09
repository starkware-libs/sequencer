use apollo_gateway_types::errors::GatewaySpecError;
use apollo_mempool_types::communication::{MempoolClientError, MempoolClientResult};
use apollo_mempool_types::errors::MempoolError;
use axum::http::StatusCode;
use blockifier::state::errors::StateError;
use serde_json::{Error as SerdeError, Value};
use starknet_api::block::GasPrice;
use starknet_api::transaction::fields::AllResourceBounds;
use starknet_api::StarknetApiError;
use thiserror::Error;
use tracing::{debug, error, warn};

use crate::compiler_version::{VersionId, VersionIdError};
use crate::rpc_objects::{RpcErrorCode, RpcErrorResponse};

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
        "Cannot declare contract class with bytecode size of {contract_bytecode_size}; max \
         allowed size: {max_contract_bytecode_size}."
    )]
    ContractBytecodeSizeTooLarge {
        contract_bytecode_size: usize,
        max_contract_bytecode_size: usize,
    },
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
    #[error("Invalid {field_name} data availability mode.")]
    InvalidDataAvailabilityMode { field_name: String },
    #[error(transparent)]
    InvalidSierraVersion(#[from] VersionIdError),
    #[error(
        "Signature length exceeded maximum: length {signature_length}
        (allowed length: {max_signature_length})."
    )]
    SignatureTooLong { signature_length: usize, max_signature_length: usize },
    #[error(transparent)]
    StarknetApiError(#[from] StarknetApiError),
    #[error(
        "Sierra versions older than {min_version} or newer than {max_version} are not supported. \
         The Sierra version of the declared contract is {version}."
    )]
    UnsupportedSierraVersion { version: VersionId, min_version: VersionId, max_version: VersionId },
    #[error("The field {field_name} should be empty.")]
    NonEmptyField { field_name: String },
    #[error(
        "At least one resource bound (L1, L2, or L1 Data) must be non-zero. Got:
        {resource_bounds:?}."
    )]
    ZeroResourceBounds { resource_bounds: AllResourceBounds },
}

impl From<StatelessTransactionValidatorError> for GatewaySpecError {
    fn from(e: StatelessTransactionValidatorError) -> Self {
        match e {
            StatelessTransactionValidatorError::ContractClassObjectSizeTooLarge { .. }
            | StatelessTransactionValidatorError::ContractBytecodeSizeTooLarge { .. } => {
                GatewaySpecError::ContractClassSizeIsTooLarge
            }
            StatelessTransactionValidatorError::UnsupportedSierraVersion { .. } => {
                GatewaySpecError::UnsupportedContractClassVersion
            }
            StatelessTransactionValidatorError::CalldataTooLong { .. }
            | StatelessTransactionValidatorError::EntryPointsNotUniquelySorted
            | StatelessTransactionValidatorError::InvalidDataAvailabilityMode { .. }
            | StatelessTransactionValidatorError::InvalidSierraVersion(..)
            | StatelessTransactionValidatorError::NonEmptyField { .. }
            | StatelessTransactionValidatorError::SignatureTooLong { .. }
            | StatelessTransactionValidatorError::StarknetApiError(..)
            | StatelessTransactionValidatorError::ZeroResourceBounds { .. } => {
                GatewaySpecError::ValidationFailure { data: e.to_string() }
            }
        }
    }
}

/// Converts a mempool client result to a gateway result. Some errors variants are unreachable in
/// Gateway context, and some are not considered errors from the gateway's perspective.
pub fn mempool_client_result_to_gw_spec_result(
    value: MempoolClientResult<()>,
) -> GatewayResult<()> {
    let err = match value {
        Ok(()) => return Ok(()),
        Err(err) => err,
    };
    match err {
        MempoolClientError::ClientError(client_error) => {
            error!("Mempool client error: {}", client_error);
            Err(GatewaySpecError::UnexpectedError { data: "Internal error".to_owned() })
        }
        MempoolClientError::MempoolError(mempool_error) => {
            debug!("Mempool error: {}", mempool_error);
            match mempool_error {
                MempoolError::DuplicateNonce { .. }
                | MempoolError::NonceTooLarge { .. }
                | MempoolError::NonceTooOld { .. } => {
                    Err(GatewaySpecError::InvalidTransactionNonce)
                }
                MempoolError::DuplicateTransaction { .. } => Err(GatewaySpecError::DuplicateTx),
                // TODO(Dafna): change to a more appropriate error, once we have it.
                MempoolError::MempoolFull { .. } => {
                    Err(GatewaySpecError::UnexpectedError { data: "Mempool full".to_owned() })
                }
                MempoolError::P2pPropagatorClientError { .. } => {
                    // Not an error from the gateway's perspective.
                    warn!("P2p propagator client error: {}", mempool_error);
                    Ok(())
                }
                MempoolError::TransactionNotFound { .. } => {
                    // This error is not expected to happen within the gateway, only from other
                    // mempool clients.
                    unreachable!("Unexpected mempool error in gateway context: {}", mempool_error);
                }
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
    #[error("Invalid params: {0:?}")]
    InvalidParams(RpcErrorResponse),
    #[error("RPC error: {0}")]
    RPCError(StatusCode),
    #[error(transparent)]
    ReqwestError(#[from] reqwest::Error),
    #[error("Unexpected error code: {0}")]
    UnexpectedErrorCode(RpcErrorCode),
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
