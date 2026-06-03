use std::net::SocketAddr;
use std::sync::Arc;

use apollo_feeder_gateway_config::config::FeederGatewayConfig;
use apollo_infra::component_definitions::ComponentStarter;
use apollo_infra_utils::type_name::short_type_name;
use async_trait::async_trait;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{middleware, serve, Extension, Router};
use tokio::net::TcpListener;
use tracing::info;

use crate::errors::FeederGatewayRunError;
use crate::reader::{AppState, ChainDataReader};

#[cfg(test)]
#[path = "feeder_gateway_test.rs"]
mod feeder_gateway_test;

pub struct FeederGateway {
    pub app_state: AppState,
}

impl FeederGateway {
    pub fn new(config: FeederGatewayConfig, reader: Arc<dyn ChainDataReader>) -> Self {
        Self { app_state: AppState { reader, config } }
    }

    pub async fn run(&mut self) -> Result<(), FeederGatewayRunError> {
        crate::metrics::init_metrics();
        let (ip, port) = self.app_state.config.ip_and_port();
        let addr = SocketAddr::new(ip, port);
        let app = self.app();
        info!("FeederGateway running on {}", addr);
        let listener = TcpListener::bind(&addr).await?;
        Ok(serve(listener, app).await?)
    }

    pub fn app(&self) -> Router {
        Router::new()
            .route(
                "/feeder_gateway/get_contract_addresses",
                get(crate::handlers::get_contract_addresses),
            )
            .route(
                "/feeder_gateway/get_block_hash_by_id",
                get(crate::handlers::get_block_hash_by_id),
            )
            .route(
                "/feeder_gateway/get_block_id_by_hash",
                get(crate::handlers::get_block_id_by_hash),
            )
            .route("/feeder_gateway/get_public_key", get(crate::handlers::get_public_key))
            .route("/feeder_gateway/get_signature", get(crate::handlers::get_signature))
            // A router layer wraps only the routes added before it, so the request metric counts
            // API requests but not the health probes below (kubernetes polls those constantly,
            // which would drown the metric in probe noise).
            .layer(middleware::from_fn(crate::metrics::record_request_metrics))
            .route(
                "/feeder_gateway/is_alive",
                get(|| async { (StatusCode::OK, "FeederGateway is alive") }),
            )
            .route(
                "/feeder_gateway/is_ready",
                get(|| async { (StatusCode::OK, "FeederGateway is ready") }),
            )
            .layer(Extension(self.app_state.clone()))
    }
}

pub fn create_feeder_gateway(
    config: FeederGatewayConfig,
    reader: Arc<dyn ChainDataReader>,
) -> FeederGateway {
    FeederGateway::new(config, reader)
}

#[async_trait]
impl ComponentStarter for FeederGateway {
    async fn start(&mut self) {
        info!("Starting component {}.", short_type_name::<Self>());
        self.run().await.unwrap_or_else(|e| panic!("Failed to start FeederGateway: {e:?}"))
    }
}
