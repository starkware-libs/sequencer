use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use blockifier::{
    blockifier::stateful_validator::StatefulValidatorError,
    transaction::errors::TransactionExecutionError,
};
use starknet_api::{
    block::BlockNumber,
    transaction::{Resource, ResourceBounds},
    StarknetApiError,
};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum GatewayError {
    #[error(transparent)]
    HTTPError(#[from] hyper::http::Error),
    #[error("Internal server error")]
    InternalServerError,
    #[error(transparent)]
    InvalidTransactionFormat(#[from] serde_json::Error),
    #[error("Error while starting the server")]
    ServerStartError(#[from] hyper::Error),
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
    ZeroResourceBounds {
        resource: Resource,
        resource_bounds: ResourceBounds,
    },
    #[error("The resource bounds mapping is missing a resource {resource:?}.")]
    MissingResource { resource: Resource },
    #[error(
        "Calldata length exceeded maximum: length {calldata_length}
        (allowed length: {max_calldata_length})."
    )]
    CalldataTooLong {
        calldata_length: usize,
        max_calldata_length: usize,
    },
    #[error(
        "Signature length exceeded maximum: length {signature_length}
        (allowed length: {max_signature_length})."
    )]
    SignatureTooLong {
        signature_length: usize,
        max_signature_length: usize,
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
    StatefulValidatorError(#[from] StatefulValidatorError),
    #[error(transparent)]
    TransactionExecutionError(#[from] TransactionExecutionError),
}

pub type StatefulTransactionValidatorResult<T> = Result<T, StatefulTransactionValidatorError>;
