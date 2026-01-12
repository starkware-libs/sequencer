//! HTTP server for the proving service.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use blockifier::state::contract_class_manager::ContractClassManager;
use blockifier::state::state_reader_and_contract_manager::StateReaderAndContractManager;
use blockifier_reexecution::state_reader::rpc_state_reader::RpcStateReader;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::core::ChainId;
use starknet_api::rpc_transaction::{
    InternalRpcInvokeTransactionV3,
    RpcInvokeTransaction,
    RpcTransaction,
};
use starknet_api::transaction::fields::{Proof, ProofFacts};
use starknet_api::transaction::{InvokeTransaction, MessageToL1, TransactionHash, TransactionHasher};
use starknet_os::io::os_output::OsOutputError;
use starknet_os::io::virtual_os_output::VirtualOsOutput;
use tracing::{info, instrument};

use crate::errors::{ProvingError, RunnerError};
use crate::prover::{prove, ProverOutput};
use crate::runner::{RpcStorageProofsProvider, RpcVirtualBlockExecutor, Runner};
use crate::server::config::ServiceConfig;

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
    pub l1_messages: Vec<MessageToL1>,
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

/// Validates that the transaction is an Invoke V3 transaction.
pub fn validate_invoke_transaction(
    tx: &RpcTransaction,
) -> Result<&RpcInvokeTransaction, HttpServerError> {
    match tx {
        RpcTransaction::Invoke(invoke) => match invoke {
            RpcInvokeTransaction::V3(_) => Ok(invoke),
        },
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
    invoke_tx: &RpcInvokeTransaction,
    chain_id: &ChainId,
) -> Result<TransactionHash, HttpServerError> {
    let internal_tx: InternalRpcInvokeTransactionV3 = match invoke_tx {
        RpcInvokeTransaction::V3(tx) => tx.clone().into(),
    };
    let version = internal_tx.version();
    internal_tx.calculate_transaction_hash(chain_id, &version).map_err(|e| {
        HttpServerError::ValidationError(format!("Failed to calculate transaction hash: {e}"))
    })
}

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    /// Contract class manager for handling compiled classes.
    pub contract_class_manager: ContractClassManager,
    /// Chain ID for transaction hash calculation.
    pub chain_id: ChainId,
    /// RPC node URL for fetching state.
    pub rpc_node_url: String,
}

/// Creates a classes provider for the given block number.
fn create_classes_provider(
    rpc_node_url: &str,
    chain_id: &ChainId,
    block_number: BlockNumber,
    contract_class_manager: ContractClassManager,
) -> Arc<StateReaderAndContractManager<RpcStateReader>> {
    let rpc_state_reader = RpcStateReader::new_with_config_from_url(
        rpc_node_url.to_string(),
        chain_id.clone(),
        block_number,
    );
    let state_reader_and_contract_manager =
        StateReaderAndContractManager::new(rpc_state_reader, contract_class_manager, None);
    Arc::new(state_reader_and_contract_manager)
}

/// Handler for the prove_transaction endpoint.
#[instrument(skip(app_state), fields(block_number, tx_hash))]
async fn prove_transaction(
    State(app_state): State<AppState>,
    Json(request): Json<ProveTransactionRequest>,
) -> Result<Json<ProveTransactionResponse>, HttpServerError> {
    let start_time = Instant::now();

    // Validate transaction type.
    let invoke_tx = validate_invoke_transaction(&request.transaction)?;

    // Calculate transaction hash.
    let tx_hash = calculate_tx_hash(invoke_tx, &app_state.chain_id)?;

    // Convert to internal transaction type.
    let invoke_transaction: InvokeTransaction = invoke_tx.clone().into();

    let block_number = BlockNumber(request.block_number);

    info!(
        block_number = %block_number,
        tx_hash = %tx_hash,
        "Starting transaction proving"
    );

    // Create per-request providers.
    let rpc_url = url::Url::parse(&app_state.rpc_node_url)
        .map_err(|e| HttpServerError::InvalidRequest(format!("Invalid RPC URL: {e}")))?;

    let virtual_block_executor = RpcVirtualBlockExecutor::new(
        app_state.rpc_node_url.clone(),
        app_state.chain_id.clone(),
        block_number,
    );
    let storage_proofs_provider = RpcStorageProofsProvider::new(rpc_url);

    // Run OS and get output.
    let os_start = Instant::now();

    // Create a runner.
    let classes_provider = create_classes_provider(
        &app_state.rpc_node_url,
        &app_state.chain_id,
        block_number,
        app_state.contract_class_manager.clone(),
    );
    let runner = Runner::new(classes_provider, storage_proofs_provider, virtual_block_executor);

    let txs = vec![(invoke_transaction, tx_hash)];
    let runner_output =
        runner.run_os(block_number, app_state.contract_class_manager.clone(), txs).await?;

    let os_duration = os_start.elapsed();
    info!(
        os_duration_ms = %os_duration.as_millis(),
        "OS execution completed"
    );

    // Parse OS output to get L1 messages.
    let virtual_os_output = VirtualOsOutput::from_raw_output(&runner_output.raw_output)?;

    // Run the prover.
    let prove_start = Instant::now();
    let prover_output: ProverOutput = prove(runner_output.cairo_pie).await?;
    let prove_duration = prove_start.elapsed();

    info!(
        prove_duration_ms = %prove_duration.as_millis(),
        total_duration_ms = %start_time.elapsed().as_millis(),
        "Proving completed"
    );

    // Build response.
    let response = ProveTransactionResponse {
        proof: prover_output.proof.into(),
        proof_facts: ProofFacts(Arc::new(prover_output.proof_facts)),
        l1_messages: virtual_os_output.messages_to_l1,
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
        let app_state = AppState {
            contract_class_manager,
            chain_id: config.chain_id.clone(),
            rpc_node_url: config.rpc_node_url.clone(),
        };
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
