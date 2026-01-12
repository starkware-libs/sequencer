//! HTTP server types for the proving service.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::fields::{Proof, ProofFacts};
use starknet_api::transaction::MessageToL1;
use starknet_os::io::os_output::OsOutputError;

use crate::errors::{ProvingError, RunnerError};

/// Request body for the prove_transaction endpoint.
#[derive(Debug, Deserialize)]
pub struct ProveTransactionRequest {
    /// The block number to execute the transaction on.
    pub block_number: u64,
    /// The transaction to prove.
    pub transaction: RpcTransaction,
}

/// Response body for the prove_transaction endpoint.
#[derive(Debug, Serialize)]
pub struct ProveTransactionResponse {
    /// The generated proof.
    pub proof: Proof,
    /// The proof facts.
    pub proof_facts: ProofFacts,
    /// Messages sent from L2 to L1 during execution.
    pub l2_to_l1_messages: Vec<MessageToL1>,
}

/// Error response body.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    /// Machine-readable error code.
    pub error_code: String,
    /// Human-readable error message.
    pub message: String,
}

/// Errors that can occur in the HTTP server.
#[derive(Debug, thiserror::Error)]
pub enum HttpServerError {
    #[error("Invalid transaction type: {0}")]
    InvalidTransactionType(String),
    #[error("Invalid request: {0}")]
    InvalidRequest(String),
    #[error("Validation error: {0}")]
    ValidationError(String),
    #[error("Runner error: {0}")]
    RunnerError(#[from] RunnerError),
    #[error("Proving error: {0}")]
    ProvingError(#[from] ProvingError),
    #[error("Output parse error: {0}")]
    OutputParseError(#[from] OsOutputError),
}

impl IntoResponse for HttpServerError {
    fn into_response(self) -> Response {
        let (status, error_code, message) = match &self {
            HttpServerError::InvalidTransactionType(msg) => {
                (StatusCode::BAD_REQUEST, "INVALID_TRANSACTION_TYPE", msg.clone())
            }
            HttpServerError::InvalidRequest(msg) => {
                (StatusCode::BAD_REQUEST, "INVALID_REQUEST", msg.clone())
            }
            HttpServerError::ValidationError(msg) => {
                (StatusCode::UNPROCESSABLE_ENTITY, "VALIDATION_ERROR", msg.clone())
            }
            HttpServerError::RunnerError(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "RUNNER_ERROR", e.to_string())
            }
            HttpServerError::ProvingError(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "PROVING_ERROR", e.to_string())
            }
            HttpServerError::OutputParseError(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "OUTPUT_PARSE_ERROR", e.to_string())
            }
        };

        let body = ErrorResponse { error_code: error_code.to_string(), message };

        (status, Json(body)).into_response()
    }
}
