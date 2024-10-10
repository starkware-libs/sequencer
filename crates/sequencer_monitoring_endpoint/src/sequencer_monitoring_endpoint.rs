use std::any::type_name;
use std::net::SocketAddr;

use axum::http::StatusCode;
use axum::routing::get;
use axum::{async_trait, Router};
use starknet_mempool_infra::component_definitions::ComponentStarter;
use starknet_mempool_infra::errors::ComponentError;
use tracing::{info, instrument};

use crate::config::SequencerMonitoringEndpointConfig;

const MONITORING_PREFIX: &str = "monitoring";

pub struct SequencerMonitoringEndpoint {
    config: SequencerMonitoringEndpointConfig,
    version: &'static str,
}

impl SequencerMonitoringEndpoint {
    pub fn new(config: SequencerMonitoringEndpointConfig, version: &'static str) -> Self {
        SequencerMonitoringEndpoint { config, version }
    }

    #[instrument(
        skip(self),
        fields(
            config = %self.config,
            version = %self.version,
        ),
        level = "debug")]
    pub async fn run(&self) -> std::result::Result<(), hyper::Error> {
        let SequencerMonitoringEndpointConfig { ip, port } = self.config;
        let endpoint_addr = SocketAddr::new(ip, port);

        let app = self.app();
        info!("SequencerMonitoringEndpoint running using socket: {}", endpoint_addr);

        axum::Server::bind(&endpoint_addr).serve(app.into_make_service()).await
    }

    fn app(&self) -> Router {
        let version = self.version.to_string();

        Router::new()
            .route(
                format!("/{MONITORING_PREFIX}/alive").as_str(),
                get(move || async { StatusCode::OK.to_string() }),
            )
            .route(
                format!("/{MONITORING_PREFIX}/ready").as_str(),
                get(move || async { StatusCode::OK.to_string() }),
            )
            .route(
                format!("/{MONITORING_PREFIX}/nodeVersion").as_str(),
                get(move || async { version }),
            )
    }
}

pub fn create_sequenser_monitoring_endpoint(
    config: SequencerMonitoringEndpointConfig,
    version: &'static str,
) -> SequencerMonitoringEndpoint {
    SequencerMonitoringEndpoint::new(config, version)
}

#[async_trait]
impl ComponentStarter for SequencerMonitoringEndpoint {
    async fn start(&mut self) -> Result<(), ComponentError> {
        info!("Starting component {}.", type_name::<Self>());
        self.run().await.map_err(|_| ComponentError::InternalComponentError)
    }
}
