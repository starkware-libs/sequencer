use std::clone::Clone;
use std::net::SocketAddr;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::post;
use axum::{async_trait, Json, Router};
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_gateway_types::communication::SharedGatewayClient;
use starknet_gateway_types::gateway_types::GatewayInput;
use starknet_infra_utils::type_name::short_type_name;
use starknet_sequencer_infra::component_definitions::ComponentStarter;
use tracing::{debug, info, instrument, trace};

use crate::config::HttpServerConfig;
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

    pub fn app(&self) -> Router {
        Router::new().route("/add_tx", post(add_tx)).with_state(self.app_state.clone())
    }
}

// HttpServer handlers.

#[instrument(skip(app_state))]
async fn add_tx(
    State(app_state): State<AppState>,
    headers: HeaderMap,
    Json(tx): Json<RpcTransaction>,
) -> HttpServerResult<Json<TransactionHash>> {
    record_added_transaction();
    let gateway_input: GatewayInput =
        GatewayInput { transactions: vec![tx], message_metadata: None };
    let add_tx_result = match app_state.gateway_client.add_txs(gateway_input).await {
        Ok(tx_hashes) => {
            assert!(tx_hashes.len() == 1, "Expected a response for exactly one transaction.");
            Ok(tx_hashes.first().unwrap().to_owned())
        }
        Err(e) => {
            debug!("Error while adding transaction: {}", e);
            Err(HttpServerError::from(e))
        }
    };

    let region =
        headers.get(CLIENT_REGION_HEADER).and_then(|region| region.to_str().ok()).unwrap_or("N/A");
    record_added_transactions(&add_tx_result, region);
    add_tx_result_as_json(add_tx_result)
}

fn record_added_transactions(add_tx_result: &HttpServerResult<TransactionHash>, region: &str) {
    if let Ok(tx_hash) = add_tx_result {
        trace!("Recorded transaction with hash: {} from region: {}", tx_hash, region);
    }
    record_added_transaction_status(add_tx_result.is_ok());
}

#[allow(clippy::result_large_err)]
pub(crate) fn add_tx_result_as_json(
    result: HttpServerResult<TransactionHash>,
) -> HttpServerResult<Json<TransactionHash>> {
    let tx_hash = result?;
    Ok(Json(tx_hash))
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
