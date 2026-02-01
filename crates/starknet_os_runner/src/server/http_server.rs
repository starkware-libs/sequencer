//! HTTP server for the proving service.
//!
//! This module provides a thin HTTP layer that delegates business logic to
//! the `VirtualSnosProver`. It handles request/response serialization, error mapping,
//! and metrics recording.

use std::net::SocketAddr;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use serde::{Deserialize, Serialize};
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::fields::{Proof, ProofFacts};
use starknet_api::transaction::MessageToL1;
use tracing::{info, instrument};

use crate::server::config::ServiceConfig;
use crate::virtual_snos_prover::{RpcVirtualSnosProver, VirtualSnosProverError};

/// Request body for the prove_transaction endpoint.
#[derive(Debug, Deserialize)]
pub struct ProveTransactionRequest {
    /// The block ID to execute the transaction on.
    pub block_id: BlockId,
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
    #[error("Invalid request: {0}")]
    InvalidRequest(String),
    #[error(transparent)]
    Prover(#[from] VirtualSnosProverError),
}

impl IntoResponse for HttpServerError {
    fn into_response(self) -> Response {
        let (status, error_code, message) = match &self {
            HttpServerError::InvalidRequest(msg) => {
                (StatusCode::BAD_REQUEST, "INVALID_REQUEST", msg.clone())
            }
            HttpServerError::Prover(e) => match e {
                VirtualSnosProverError::InvalidTransactionType(msg) => {
                    (StatusCode::BAD_REQUEST, "INVALID_TRANSACTION_TYPE", msg.clone())
                }
                VirtualSnosProverError::ValidationError(msg) => {
                    (StatusCode::UNPROCESSABLE_ENTITY, "VALIDATION_ERROR", msg.clone())
                }
                VirtualSnosProverError::TransactionHashError(msg) => {
                    (StatusCode::UNPROCESSABLE_ENTITY, "TRANSACTION_HASH_ERROR", msg.clone())
                }
                VirtualSnosProverError::RunnerError(err) => {
                    (StatusCode::INTERNAL_SERVER_ERROR, "RUNNER_ERROR", err.to_string())
                }
                VirtualSnosProverError::ProvingError(err) => {
                    (StatusCode::INTERNAL_SERVER_ERROR, "PROVING_ERROR", err.to_string())
                }
                VirtualSnosProverError::OutputParseError(err) => {
                    (StatusCode::INTERNAL_SERVER_ERROR, "OUTPUT_PARSE_ERROR", err.to_string())
                }
                VirtualSnosProverError::ProgramOutputError(e) => {
                    (StatusCode::INTERNAL_SERVER_ERROR, "PROGRAM_OUTPUT_ERROR", e.to_string())
                }
            },
        };

        let body = ErrorResponse { error_code: error_code.to_string(), message };

        (status, Json(body)).into_response()
    }
}

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    /// The prover that handles business logic.
    pub(crate) prover: RpcVirtualSnosProver,
}

/// Handler for the prove_transaction endpoint.
#[instrument(skip(app_state), fields(block_id))]
async fn prove_transaction(
    State(app_state): State<AppState>,
    Json(request): Json<ProveTransactionRequest>,
) -> Result<Json<ProveTransactionResponse>, HttpServerError> {
    // Delegate to the prover.
    let output = app_state.prover.prove_transaction(request.block_id, request.transaction).await?;

    // Build response.
    let response = ProveTransactionResponse {
        proof: output.proof,
        proof_facts: output.proof_facts,
        l2_to_l1_messages: output.l2_to_l1_messages,
    };

    Ok(Json(response))
}

/// Creates the router with all endpoints.
pub fn create_router(app_state: AppState) -> Router {
    Router::new().route("/prove_transaction", post(prove_transaction)).with_state(app_state)
}

/// The HTTP proving server.
pub struct ProvingHttpServer {
    config: ServiceConfig,
    app_state: AppState,
}

impl ProvingHttpServer {
    /// Creates a new ProvingHttpServer.
    pub fn new(config: ServiceConfig) -> Self {
        let prover = RpcVirtualSnosProver::new(&config);
        let app_state = AppState { prover };
        Self { config, app_state }
    }

    /// Runs the server.
    pub async fn run(&self) -> Result<(), hyper::Error> {
        let addr = SocketAddr::new(self.config.ip, self.config.port);
        let app = create_router(self.app_state.clone());
        info!("ProvingHttpServer running on {}", addr);
        axum::Server::bind(&addr).serve(app.into_make_service()).await
    }
}
