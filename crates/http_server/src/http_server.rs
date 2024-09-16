use std::clone::Clone;
use std::net::SocketAddr;

use async_trait::async_trait;
use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_gateway::errors::GatewayRunError;
use starknet_gateway_types::communication::SharedGatewayClient;
use starknet_gateway_types::errors::GatewaySpecError;
use starknet_gateway_types::gateway_types::{GatewayInput, MessageMetadata};
use starknet_mempool_infra::component_runner::{ComponentStartError, ComponentStarter};
use tracing::{error, info, instrument};

use crate::config::HttpServerConfig;

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

    pub async fn run(&mut self) -> Result<(), GatewayRunError> {
        // Parses the bind address from HttpServerConfig, returning an error for invalid addresses.
        let HttpServerConfig { ip, port } = self.config;
        let addr = SocketAddr::new(ip, port);
        let app = self.app();

        // Create a server that runs forever.
        Ok(axum::Server::bind(&addr).serve(app.into_make_service()).await?)
    }

    pub fn app(&self) -> Router {
        Router::new()
            .route("/is_alive", get(is_alive))
            .route("/add_tx", post(add_tx))
            .with_state(self.app_state.clone())
    }
}

// HttpServer handlers.

#[instrument]
async fn is_alive() -> HttpServerResult<String> {
    unimplemented!("Future handling should be implemented here.");
}

#[instrument(skip(app_state))]
async fn add_tx(
    State(app_state): State<AppState>,
    Json(tx): Json<RpcTransaction>,
) -> HttpServerResult<Json<TransactionHash>> {
    let gateway_input: GatewayInput =
        GatewayInput { rpc_tx: tx.clone(), message_metadata: MessageMetadata {} };

    let tx_hash = app_state.gateway_client.add_tx(gateway_input).await.map_err(|join_err| {
        error!("Failed to process tx: {}", join_err);
        GatewaySpecError::UnexpectedError { data: "Internal server error".to_owned() }
    })?;

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
    async fn start(&mut self) -> Result<(), ComponentStartError> {
        info!("HttpServer::start()");
        self.run().await.map_err(|_| ComponentStartError::InternalComponentError)
    }
}
