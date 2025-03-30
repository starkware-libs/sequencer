use std::clone::Clone;
use std::net::SocketAddr;
use std::string::String;

use apollo_gateway_types::communication::SharedGatewayClient;
use apollo_gateway_types::gateway_types::{GatewayInput, GatewayOutput};
use apollo_infra_utils::type_name::short_type_name;
use apollo_sequencer_infra::component_definitions::ComponentStarter;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::post;
use axum::{async_trait, Json, Router};
use serde::{Deserialize, Serialize};
use starknet_api::core::{ClassHash, ContractAddress};
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;
use tracing::{debug, info, instrument, trace};

use crate::config::HttpServerConfig;
use crate::deprecated_gateway_transaction::DeprecatedGatewayTransactionV3;
use crate::errors::{HttpServerError, HttpServerRunError};
use crate::metrics::{init_metrics, record_added_transaction, record_added_transaction_status};

#[cfg(test)]
#[path = "http_server_test.rs"]
pub mod http_server_test;

pub type HttpServerResult<T> = Result<T, HttpServerError>;

const CLIENT_REGION_HEADER: &str = "X-Client-Region";

pub struct HttpServer {
    pub config: HttpServerConfig,
    app_state: AppState,
}

#[derive(Clone)]
pub struct AppState {
    pub gateway_client: SharedGatewayClient,
}

impl HttpServer {
    pub fn new(config: HttpServerConfig, gateway_client: SharedGatewayClient) -> Self {
        let app_state = AppState { gateway_client };
        init_metrics();
        HttpServer { config, app_state }
    }

    pub async fn run(&mut self) -> Result<(), HttpServerRunError> {
        // Parses the bind address from HttpServerConfig, returning an error for invalid addresses.
        let HttpServerConfig { ip, port } = self.config;
        let addr = SocketAddr::new(ip, port);
        let app = self.app();
        info!("HttpServer running using socket: {}", addr);

        // Create a server that runs forever.
        Ok(axum::Server::bind(&addr).serve(app.into_make_service()).await?)
    }

    // TODO(Yael): consider supporting both formats in the same endpoint if possible.
    pub fn app(&self) -> Router {
        Router::new()
            // Json Rpc endpoint
            .route("/add_rpc_transaction", post(add_rpc_tx))
            .with_state(self.app_state.clone())
            // Rest api endpoint
            .route("/add_transaction", post(add_tx))
            .with_state(self.app_state.clone())
    }
}

// HttpServer handlers.

#[instrument(skip(app_state))]
async fn add_rpc_tx(
    State(app_state): State<AppState>,
    headers: HeaderMap,
    Json(tx): Json<RpcTransaction>,
) -> HttpServerResult<Json<GatewayResponse>> {
    record_added_transaction();
    add_tx_inner(app_state, headers, tx).await
}

#[instrument(skip(app_state))]
async fn add_tx(
    State(app_state): State<AppState>,
    headers: HeaderMap,
    tx: String,
) -> HttpServerResult<Json<GatewayResponse>> {
    record_added_transaction();
    // TODO(Yael): increment the failure metric for parsing error.
    let tx: DeprecatedGatewayTransactionV3 = serde_json::from_str(&tx)
        .inspect_err(|e| debug!("Error while parsing transaction: {}", e))?;
    let rpc_tx = tx.try_into().inspect_err(|e| {
        debug!("Error while converting deprecated gateway transaction into RPC transaction: {}", e);
    })?;

    add_tx_inner(app_state, headers, rpc_tx).await
}

async fn add_tx_inner(
    app_state: AppState,
    headers: HeaderMap,
    tx: RpcTransaction,
) -> HttpServerResult<Json<GatewayResponse>> {
    let gateway_input: GatewayInput = GatewayInput { rpc_tx: tx, message_metadata: None };
    let add_tx_result = app_state.gateway_client.add_tx(gateway_input).await.map_err(|e| {
        debug!("Error while adding transaction: {}", e);
        HttpServerError::from(e)
    });

    let region =
        headers.get(CLIENT_REGION_HEADER).and_then(|region| region.to_str().ok()).unwrap_or("N/A");
    record_added_transactions(&add_tx_result, region);
    add_tx_result_as_json(add_tx_result)
}

fn record_added_transactions(add_tx_result: &HttpServerResult<GatewayOutput>, region: &str) {
    if let Ok(gateway_output) = add_tx_result {
        trace!(
            "Recorded transaction with hash: {} from region: {}",
            gateway_output.transaction_hash(),
            region
        );
    }
    record_added_transaction_status(add_tx_result.is_ok());
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct GatewayResponse {
    code: String,
    transaction_hash: TransactionHash,
    // This parameter should be Some if and only if it is a positive response to a Declare
    // transaction.
    class_hash: Option<ClassHash>,
    // This parameter should be Some if and only if it is a positive response to a Deploy account
    // Transaction.
    address: Option<ContractAddress>,
}

impl Default for GatewayResponse {
    fn default() -> Self {
        GatewayResponse {
            code: Self::TRANSACTION_RECEIVED.to_string(),
            transaction_hash: TransactionHash::default(),
            class_hash: None,
            address: None,
        }
    }
}

impl GatewayResponse {
    pub fn transaction_hash(&self) -> TransactionHash {
        self.transaction_hash
    }

    const TRANSACTION_RECEIVED: &'static str = "TRANSACTION_RECEIVED";
}

#[allow(clippy::result_large_err)]
pub(crate) fn add_tx_result_as_json(
    result: HttpServerResult<GatewayOutput>,
) -> HttpServerResult<Json<GatewayResponse>> {
    let gateway_output = result?;
    let transaction_hash = gateway_output.transaction_hash();
    let mut response = GatewayResponse { transaction_hash, ..Default::default() };

    if let GatewayOutput::Declare(declare_output) = &gateway_output {
        response.class_hash = Some(declare_output.class_hash);
    }
    if let GatewayOutput::DeployAccount(deploy_account_output) = &gateway_output {
        response.address = Some(deploy_account_output.address);
    }

    Ok(Json(response))
}

pub fn create_http_server(
    config: HttpServerConfig,
    gateway_client: SharedGatewayClient,
) -> HttpServer {
    HttpServer::new(config, gateway_client)
}

#[async_trait]
impl ComponentStarter for HttpServer {
    async fn start(&mut self) {
        info!("Starting component {}.", short_type_name::<Self>());
        self.run().await.unwrap_or_else(|e| panic!("Failed to start HttpServer component: {:?}", e))
    }
}
