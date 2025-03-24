use std::clone::Clone;
use std::net::SocketAddr;
use std::string::String;

use apollo_gateway_types::communication::SharedGatewayClient;
use apollo_gateway_types::gateway_types::{GatewayInput, GatewayOutput};
use apollo_infra::component_definitions::ComponentStarter;
use apollo_infra_utils::type_name::short_type_name;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{async_trait, Json, Router};
use starknet_api::rpc_transaction::RpcTransaction;
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
            .route("/gateway/add_rpc_transaction", post(add_rpc_tx))
            .with_state(self.app_state.clone())
            // Rest api endpoint
            .route("/gateway/add_transaction", post(add_tx))
            .with_state(self.app_state.clone())
            .route("/gateway/is_alive", get(is_alive))
            .route("/gateway/is_ready", get(is_ready))
    }
}

pub async fn is_ready() -> String {
    "Gateway is ready!".to_string()
}

pub async fn is_alive() -> String {
    "Gateway is alive!".to_string()
}

// HttpServer handlers.

#[instrument(skip(app_state))]
async fn add_rpc_tx(
    State(app_state): State<AppState>,
    headers: HeaderMap,
    Json(tx): Json<RpcTransaction>,
) -> HttpServerResult<Json<GatewayOutput>> {
    record_added_transaction();
    add_tx_inner(app_state, headers, tx).await
}

#[instrument(skip(app_state))]
async fn add_tx(
    State(app_state): State<AppState>,
    headers: HeaderMap,
    tx: String,
) -> HttpServerResult<Json<GatewayOutput>> {
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
) -> HttpServerResult<Json<GatewayOutput>> {
    let gateway_input: GatewayInput = GatewayInput { rpc_tx: tx, message_metadata: None };
    let add_tx_result = app_state.gateway_client.add_tx(gateway_input).await.map_err(|e| {
        debug!("Error while adding transaction: {}", e);
        HttpServerError::from(e)
    });

    let region =
        headers.get(CLIENT_REGION_HEADER).and_then(|region| region.to_str().ok()).unwrap_or("N/A");
    record_added_transactions(&add_tx_result, region);
    Ok(Json(add_tx_result?))
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
