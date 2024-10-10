use std::any::type_name;
use std::collections::BTreeMap;
use std::fmt::Display;
use std::net::SocketAddr;
use std::str::FromStr;

use axum::http::StatusCode;
use axum::routing::get;
use axum::{async_trait, Router};
use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_mempool_infra::component_definitions::ComponentStarter;
use starknet_mempool_infra::errors::ComponentError;
use tracing::{debug, info, instrument};
use validator::Validate;

const MONITORING_PREFIX: &str = "monitoring";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Validate)]
pub struct SequencerMonitoringEndpointConfig {
    pub endpoint_address: String,
}

impl Default for SequencerMonitoringEndpointConfig {
    fn default() -> Self {
        SequencerMonitoringEndpointConfig { endpoint_address: String::from("0.0.0.0:8082") }
    }
}

impl SerializeConfig for SequencerMonitoringEndpointConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([ser_param(
            "server_address",
            &self.endpoint_address,
            "node's monitoring server.",
            ParamPrivacyInput::Public,
        )])
    }
}

impl Display for SequencerMonitoringEndpointConfig {
    #[cfg_attr(coverage_nightly, coverage_attribute)]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

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
        let server_address = SocketAddr::from_str(&self.config.endpoint_address).expect(
            "Configuration value for sequencer monitoring endpoint address should be valid",
        );
        let app = app(self.version);
        debug!("Starting sequencer monitoring endpoint.");
        axum::Server::bind(&server_address).serve(app.into_make_service()).await
    }
}

#[allow(clippy::too_many_arguments)]
fn app(version: &'static str) -> Router {
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
            get(move || async { version.to_string() }),
        )
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
