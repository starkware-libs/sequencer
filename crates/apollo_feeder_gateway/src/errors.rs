use std::io;

use apollo_gateway_types::deprecated_gateway_error::{
    KnownStarknetErrorCode,
    StarknetError,
    StarknetErrorCode,
};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use thiserror::Error;

use crate::serialization::to_python_json;

#[cfg(test)]
#[path = "errors_test.rs"]
mod errors_test;

#[derive(Debug, Error)]
pub enum FeederGatewayRunError {
    #[error(transparent)]
    ServerStartupError(#[from] io::Error),
}

/// Errors returned by feeder gateway request handling. Each maps to the legacy Python feeder
/// gateway error envelope (`{code, message}`) and HTTP status (see [`IntoResponse`]).
#[derive(Debug, Error)]
pub enum FeederGatewayError {
    #[error("Block not found")]
    BlockNotFound,
    #[error("Transaction hash not found")]
    TransactionNotFound,
    #[error("Malformed request: {0}")]
    MalformedRequest(String),
    // The source of an internal error is logged at the construction site and deliberately not
    // carried here, so nothing internal leaks to the client.
    #[error("Internal error")]
    Internal,
}

impl IntoResponse for FeederGatewayError {
    fn into_response(self) -> Response {
        // Status mapping mirrors the Python feeder gateway: a `StarknetErrorCode` body is HTTP 400
        // (verified against the live feeder gateway: BLOCK_NOT_FOUND returns 400, not 404), and
        // only unhandled internal errors are 500.
        let (status, starknet_error) = match self {
            FeederGatewayError::BlockNotFound => (
                StatusCode::BAD_REQUEST,
                StarknetError {
                    code: StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::BlockNotFound),
                    message: "Block not found".to_string(),
                },
            ),
            FeederGatewayError::TransactionNotFound => (
                StatusCode::BAD_REQUEST,
                StarknetError {
                    code: StarknetErrorCode::UnknownErrorCode(
                        "StarknetErrorCode.TRANSACTION_NOT_FOUND".to_string(),
                    ),
                    message: "Transaction hash not found".to_string(),
                },
            ),
            FeederGatewayError::MalformedRequest(message) => (
                StatusCode::BAD_REQUEST,
                StarknetError {
                    code: StarknetErrorCode::KnownErrorCode(
                        KnownStarknetErrorCode::MalformedRequest,
                    ),
                    message,
                },
            ),
            FeederGatewayError::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                StarknetError {
                    code: StarknetErrorCode::UnknownErrorCode(
                        "StarknetErrorCode.INTERNAL_ERROR".to_string(),
                    ),
                    message: "Internal error".to_string(),
                },
            ),
        };
        serialize_error(status, &starknet_error)
    }
}

/// Serializes a [`StarknetError`] into a byte-parity error response: the spaced Python formatter
/// (not `serde_json::to_vec`) so the envelope is byte-exact. The message is NOT sanitized: the
/// live feeder gateway echoes request values verbatim in messages (verified 2026-06-03 with
/// quotes/`;`/`<>` inputs), relying only on JSON string escaping; the `apollo_http_server`
/// sanitizer belongs to the write gateway, not here.
fn serialize_error(status: StatusCode, error: &StarknetError) -> Response {
    let body = to_python_json(error).expect("StarknetError is serializable");
    (status, [(header::CONTENT_TYPE, "application/json")], body).into_response()
}
