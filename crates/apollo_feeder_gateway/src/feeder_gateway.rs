use std::net::SocketAddr;

use apollo_feeder_gateway_config::config::FeederGatewayConfig;
use apollo_infra::component_definitions::ComponentStarter;
use apollo_infra_utils::type_name::short_type_name;
use async_trait::async_trait;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{serve, Router};
use tokio::net::TcpListener;
use tracing::info;

use crate::errors::FeederGatewayRunError;

#[cfg(test)]
#[path = "feeder_gateway_test.rs"]
mod feeder_gateway_test;

pub struct FeederGateway {
    pub config: FeederGatewayConfig,
}

impl FeederGateway {
    pub fn new(config: FeederGatewayConfig) -> Self {
        Self { config }
    }

    pub async fn run(&mut self) -> Result<(), FeederGatewayRunError> {
        let (ip, port) = self.config.ip_and_port();
        let addr = SocketAddr::new(ip, port);
        let app = self.app();
        info!("FeederGateway running on {}", addr);
        let listener = TcpListener::bind(&addr).await?;
        Ok(serve(listener, app).await?)
    }

    pub fn app(&self) -> Router {
        Router::new()
            .route(
                "/feeder_gateway/is_alive",
                get(|| async { (StatusCode::OK, "FeederGateway is alive") }),
            )
            .route(
                "/feeder_gateway/is_ready",
                get(|| async { (StatusCode::OK, "FeederGateway is ready") }),
            )
    }
}

pub fn create_feeder_gateway(config: FeederGatewayConfig) -> FeederGateway {
    FeederGateway::new(config)
}

#[async_trait]
impl ComponentStarter for FeederGateway {
    async fn start(&mut self) {
        info!("Starting component {}.", short_type_name::<Self>());
        self.run().await.unwrap_or_else(|e| panic!("Failed to start FeederGateway: {e:?}"))
    }
}
