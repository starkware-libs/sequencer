//! HTTP server for the proving service.

use std::net::SocketAddr;
use std::time::Instant;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use blockifier::state::contract_class_manager::ContractClassManager;
use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use serde::{Deserialize, Serialize};
use starknet_api::core::ChainId;
use starknet_api::rpc_transaction::{RpcInvokeTransaction, RpcTransaction};
use starknet_api::transaction::fields::{Proof, ProofFacts, VIRTUAL_SNOS};
use starknet_api::transaction::{
    InvokeTransaction,
    MessageToL1,
    TransactionHash,
    TransactionHasher,
};
use starknet_api::StarknetApiError;
use starknet_os::io::os_output::OsOutputError;
use starknet_types_core::felt::Felt;
use tracing::{info, instrument};
use url::Url;

use crate::errors::{ProvingError, RunnerError};
use crate::proving::prover::{prove, ProverOutput};
use crate::runner::RpcRunnerFactory;
use crate::server::config::ServiceConfig;

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
    #[error(transparent)]
    StarknetApiError(#[from] StarknetApiError),
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
            HttpServerError::StarknetApiError(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "STARKNET_API_ERROR", e.to_string())
            }
        };

        let body = ErrorResponse { error_code: error_code.to_string(), message };

        (status, Json(body)).into_response()
    }
}

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

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    /// Factory for creating RPC-based runners.
    pub(crate) runner_factory: RpcRunnerFactory,
    /// Chain ID for transaction hash calculation.
    pub chain_id: ChainId,
}

/// Handler for the prove_transaction endpoint.
#[instrument(skip(app_state), fields(block_id, tx_hash))]
async fn prove_transaction(
    State(app_state): State<AppState>,
    Json(request): Json<ProveTransactionRequest>,
) -> Result<Json<ProveTransactionResponse>, HttpServerError> {
    let start_time = Instant::now();

    let invoke_tx = extract_invoke_tx(request.transaction)?;

    let tx_hash = calculate_tx_hash(&invoke_tx, &app_state.chain_id)?;

    info!(
        block_id = ?request.block_id,
        tx_hash = %tx_hash,
        "Starting transaction proving"
    );

    // Run OS and get output.
    let os_start = Instant::now();

    // Create a runner using the factory.
    let runner = app_state.runner_factory.create_runner(request.block_id);

    let txs = vec![(invoke_tx, tx_hash)];
    let runner_output = runner
        .run_virtual_os(txs)
        .await
        .map_err(|err| HttpServerError::RunnerError(Box::new(err)))?;

    let os_duration = os_start.elapsed();
    info!(
        os_duration_ms = %os_duration.as_millis(),
        "OS execution completed"
    );

    // Run the prover.
    let prove_start = Instant::now();
    let prover_output: ProverOutput = prove(runner_output.cairo_pie).await?;
    let prove_duration = prove_start.elapsed();

    info!(
        prove_duration_ms = %prove_duration.as_millis(),
        total_duration_ms = %start_time.elapsed().as_millis(),
        "Proving completed"
    );

    // Convert program output to proof facts using VIRTUAL_SNOS variant marker.
    let proof_facts = prover_output.program_output.to_proof_facts(Felt::from(VIRTUAL_SNOS))?;

    // Build response.
    let response = ProveTransactionResponse {
        proof: prover_output.proof,
        proof_facts,
        // TODO(Aviv): Add l2_to_l1_messages to the runner output and use it here.
        l2_to_l1_messages: Vec::new(),
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
        let contract_class_manager =
            ContractClassManager::start(config.contract_class_manager_config.clone());
        let node_url = Url::parse(&config.rpc_node_url).expect("Invalid RPC node URL in config");
        let runner_factory =
            RpcRunnerFactory::new(node_url, config.chain_id.clone(), contract_class_manager);
        let app_state = AppState { runner_factory, chain_id: config.chain_id.clone() };
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
