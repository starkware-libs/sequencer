//! HTTP server types for the proving service.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use starknet_api::core::ChainId;
use starknet_api::rpc_transaction::{RpcInvokeTransaction, RpcTransaction};
use starknet_api::transaction::fields::{Proof, ProofFacts};
use starknet_api::transaction::{
    InvokeTransaction,
    MessageToL1,
    TransactionHash,
    TransactionHasher,
};
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
    #[error(transparent)]
    // Boxed to reduce the size of Result on the stack (RunnerError is >128 bytes).
    RunnerError(#[from] Box<RunnerError>),
    #[error(transparent)]
    ProvingError(#[from] ProvingError),
    #[error(transparent)]
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

#[allow(dead_code)]
/// Validates that the transaction is an Invoke transaction and extracts it.
fn extract_invoke_tx(tx: RpcTransaction) -> Result<InvokeTransaction, HttpServerError> {
    match tx {
        RpcTransaction::Invoke(RpcInvokeTransaction::V3(invoke_v3)) => {
            Ok(InvokeTransaction::V3(invoke_v3.into()))
        }
        RpcTransaction::Declare(_) => Err(HttpServerError::InvalidTransactionType(
            "Declare transactions are not supported; only Invoke transactions are allowed"
                .to_string(),
        )),
        RpcTransaction::DeployAccount(_) => Err(HttpServerError::InvalidTransactionType(
            "DeployAccount transactions are not supported; only Invoke transactions are allowed"
                .to_string(),
        )),
    }
}

#[allow(dead_code)]
/// Calculates the transaction hash for an invoke transaction.
fn calculate_tx_hash(
    invoke_tx: &InvokeTransaction,
    chain_id: &ChainId,
) -> Result<TransactionHash, HttpServerError> {
    let version = invoke_tx.version();
    invoke_tx.calculate_transaction_hash(chain_id, &version).map_err(|e| {
        HttpServerError::ValidationError(format!("Failed to calculate transaction hash: {e}"))
    })
}
