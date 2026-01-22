//! HTTP server for the proving service.
//!
//! This module provides a thin HTTP layer that delegates business logic to
//! the `VirtualSnosProver`. It handles request/response serialization, error mapping,
//! and metrics recording.

use std::net::SocketAddr;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use serde::{Deserialize, Serialize};
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::fields::{Proof, ProofFacts};
use starknet_api::transaction::MessageToL1;
use starknet_rust::providers::jsonrpc::HttpTransport;
use starknet_rust::providers::{JsonRpcClient, Provider};
use tracing::{info, instrument};

use crate::proving::prover::{resolve_resource_path, BOOTLOADER_FILE};
use crate::server::config::ServiceConfig;
use crate::virtual_snos_prover::{VirtualSnosProver, VirtualSnosProverError};

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
                VirtualSnosProverError::RunnerError(err) => {
                    (StatusCode::INTERNAL_SERVER_ERROR, "RUNNER_ERROR", err.to_string())
                }
                VirtualSnosProverError::ProvingError(err) => {
                    (StatusCode::INTERNAL_SERVER_ERROR, "PROVING_ERROR", err.to_string())
                }
                VirtualSnosProverError::OutputParseError(err) => {
                    (StatusCode::INTERNAL_SERVER_ERROR, "OUTPUT_PARSE_ERROR", err.to_string())
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
    pub(crate) prover: VirtualSnosProver,
}

#[derive(Debug, Serialize)]
struct CheckStatus {
    name: &'static str,
    ok: bool,
    message: Option<String>,
}

#[derive(Debug, Serialize)]
struct ServiceStatusResponse {
    status: &'static str,
    checks: Vec<CheckStatus>,
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

/// Handler for the is_alive (liveness) endpoint.
///
/// Returns 200 OK if local resources required for proving are available.
/// This avoids marking the service healthy when required files are missing.
async fn is_alive() -> impl IntoResponse {
    let checks = vec![check_bootloader_file()];
    build_status_response(checks, StatusCode::INTERNAL_SERVER_ERROR)
}

/// Handler for the is_ready (readiness) endpoint.
///
/// Returns 200 OK if the server is ready to accept requests.
/// This checks external dependencies needed for serving requests.
async fn is_ready(State(app_state): State<AppState>) -> impl IntoResponse {
    let mut checks = vec![check_bootloader_file()];
    checks.extend(check_rpc_checks(&app_state.prover).await);
    build_status_response(checks, StatusCode::SERVICE_UNAVAILABLE)
}

fn check_bootloader_file() -> CheckStatus {
    match resolve_resource_path(BOOTLOADER_FILE) {
        Ok(_) => CheckStatus { name: "bootloader_file", ok: true, message: None },
        Err(err) => CheckStatus {
            name: "bootloader_file",
            ok: false,
            message: Some(format!("Bootloader file check failed: {err}")),
        },
    }
}

async fn check_rpc_checks(prover: &VirtualSnosProver) -> Vec<CheckStatus> {
    let client = JsonRpcClient::new(HttpTransport::new(prover.rpc_url().clone()));
    match client.chain_id().await {
        Ok(chain_id) => {
            let availability_check = CheckStatus { name: "rpc_available", ok: true, message: None };
            let expected = prover.chain_id().as_hex().to_lowercase();
            let actual = chain_id.to_hex_string().to_lowercase();
            let chain_id_check = if actual == expected {
                CheckStatus { name: "rpc_chain_id", ok: true, message: None }
            } else {
                CheckStatus {
                    name: "rpc_chain_id",
                    ok: false,
                    message: Some(format!(
                        "RPC chain id {actual} does not match expected {expected}"
                    )),
                }
            };
            vec![availability_check, chain_id_check]
        }
        Err(err) => vec![
            CheckStatus {
                name: "rpc_available",
                ok: false,
                message: Some(format!("RPC unavailable: {err}")),
            },
            CheckStatus {
                name: "rpc_chain_id",
                ok: false,
                message: Some(format!("RPC chain id check skipped: RPC unavailable: {err}")),
            },
        ],
    }
}

fn build_status_response(
    checks: Vec<CheckStatus>,
    failure_status: StatusCode,
) -> (StatusCode, Json<ServiceStatusResponse>) {
    let all_ok = checks.iter().all(|check| check.ok);
    let status_code = if all_ok { StatusCode::OK } else { failure_status };
    let status = if all_ok { "ok" } else { "error" };
    (status_code, Json(ServiceStatusResponse { status, checks }))
}

/// Creates the router with all endpoints.
pub fn create_router(app_state: AppState) -> Router {
    Router::new()
        .route("/prove_transaction", post(prove_transaction))
        .route("/gateway/is_alive", get(is_alive))
        .route("/gateway/is_ready", get(is_ready))
        .with_state(app_state)
}

/// The HTTP proving server.
pub struct ProvingHttpServer {
    config: ServiceConfig,
    app_state: AppState,
}

impl ProvingHttpServer {
    /// Creates a new ProvingHttpServer.
    pub fn new(config: ServiceConfig) -> Self {
        let prover = VirtualSnosProver::new(&config);
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
