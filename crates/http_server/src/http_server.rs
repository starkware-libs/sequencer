use std::any::type_name;
use std::clone::Clone;
use std::net::SocketAddr;

use axum::extract::State;
use axum::routing::post;
use axum::{async_trait, Json, Router};
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_gateway_types::communication::SharedGatewayClient;
use starknet_gateway_types::errors::GatewaySpecError;
use starknet_gateway_types::gateway_types::GatewayInput;
use starknet_mempool_infra::component_definitions::ComponentStarter;
use starknet_mempool_infra::errors::ComponentError;
use tracing::{error, info, instrument};

use crate::config::HttpServerConfig;
use crate::errors::HttpServerRunError;

#[cfg(test)]
#[path = "http_server_test.rs"]
pub mod http_server_test;

pub type HttpServerResult<T> = Result<T, GatewaySpecError>;

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
    Json(tx): Json<RpcTransaction>,
) -> HttpServerResult<Json<TransactionHash>> {
    info!("Received tx: {:?}", tx);
    let gateway_input = GatewayInput { rpc_tx: tx, message_metadata: None };
    let add_tx_result = app_state.gateway_client.add_tx(gateway_input).await.map_err(|join_err| {
        error!("Failed to process tx: {}", join_err);
        GatewaySpecError::UnexpectedError { data: "Internal server error".to_owned() }
    });
    add_tx_result_as_json(add_tx_result)
}

pub(crate) fn add_tx_result_as_json(
    result: Result<TransactionHash, GatewaySpecError>,
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
    async fn start(&mut self) -> Result<(), ComponentError> {
        info!("Starting component {}.", type_name::<Self>());
        self.run().await.map_err(|_| ComponentError::InternalComponentError)
    }
}
