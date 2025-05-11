use std::net::SocketAddr;
use std::str::FromStr;

use apollo_infra::component_definitions::ComponentStarter;
use apollo_infra::trace_util::change_tracing_level;
use apollo_infra_utils::type_name::short_type_name;
use apollo_mempool_types::communication::SharedMempoolClient;
use apollo_mempool_types::mempool_types::MempoolSnapshot;
use apollo_metrics::metrics::COLLECT_SEQUENCER_PROFILING_METRICS;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, put};
use axum::{async_trait, Json, Router, Server};
use hyper::Error;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use tracing::metadata::LevelFilter;
use tracing::{error, info, instrument};

use crate::config::MonitoringEndpointConfig;

#[cfg(test)]
#[path = "monitoring_endpoint_test.rs"]
mod monitoring_endpoint_test;

pub(crate) const MONITORING_PREFIX: &str = "monitoring";
pub(crate) const ALIVE: &str = "alive";
pub(crate) const READY: &str = "ready";
pub(crate) const VERSION: &str = "nodeVersion";
pub(crate) const METRICS: &str = "metrics";
pub(crate) const MEMPOOL_SNAPSHOT: &str = "mempoolSnapshot";
pub(crate) const SET_GLOBAL_LOG_LEVEL: &str = "setGlobalLogLevel";

pub struct MonitoringEndpoint {
    config: MonitoringEndpointConfig,
    version: &'static str,
    prometheus_handle: Option<PrometheusHandle>,
    mempool_client: Option<SharedMempoolClient>,
}

impl MonitoringEndpoint {
    pub fn new(
        config: MonitoringEndpointConfig,
        version: &'static str,
        mempool_client: Option<SharedMempoolClient>,
    ) -> Self {
        // TODO(Tsabary): consider error handling
        let prometheus_handle = if config.collect_metrics {
            // TODO(Lev): add tests that show the metrics are collected / not collected based on the
            // config value.
            COLLECT_SEQUENCER_PROFILING_METRICS
                .set(config.collect_profiling_metrics)
                .expect("Should be able to set profiling metrics collection.");

            Some(
                PrometheusBuilder::new()
                    .install_recorder()
                    .expect("should be able to build the recorder and install it globally"),
            )
        } else {
            None
        };
        MonitoringEndpoint { config, version, prometheus_handle, mempool_client }
    }

    #[instrument(
        skip(self),
        fields(
            config = %self.config,
            version = %self.version,
        ),
        level = "debug")]
    pub async fn run(&self) -> Result<(), Error> {
        let MonitoringEndpointConfig { ip, port, .. } = self.config;
        let endpoint_addr = SocketAddr::new(ip, port);

        let app = self.app();
        info!("MonitoringEndpoint running using socket: {}", endpoint_addr);

        Server::bind(&endpoint_addr).serve(app.into_make_service()).await
    }

    fn app(&self) -> Router {
        let version = self.version.to_string();
        let prometheus_handle = self.prometheus_handle.clone();
        let mempool_client = self.mempool_client.clone();

        Router::new()
            .route(
                format!("/{MONITORING_PREFIX}/{ALIVE}").as_str(),
                get(move || async { StatusCode::OK.to_string() }),
            )
            .route(
                format!("/{MONITORING_PREFIX}/{READY}").as_str(),
                get(move || async { StatusCode::OK.to_string() }),
            )
            .route(
                format!("/{MONITORING_PREFIX}/{VERSION}").as_str(),
                get(move || async { version }),
            )
            .route(
                format!("/{MONITORING_PREFIX}/{METRICS}").as_str(),
                get(move || metrics(prometheus_handle)),
            )
            .route(
                format!("/{MONITORING_PREFIX}/{MEMPOOL_SNAPSHOT}").as_str(),
                get(move || mempool_snapshot(mempool_client)),
            )
            .route(
                format!("/{MONITORING_PREFIX}/{SET_GLOBAL_LOG_LEVEL}/:logLevel").as_str(),
                put(change_global_log_level),
            )
    }
}

pub fn create_monitoring_endpoint(
    config: MonitoringEndpointConfig,
    version: &'static str,
    mempool_client: Option<SharedMempoolClient>,
) -> MonitoringEndpoint {
    MonitoringEndpoint::new(config, version, mempool_client)
}

#[async_trait]
impl ComponentStarter for MonitoringEndpoint {
    async fn start(&mut self) {
        info!("Starting component {}.", short_type_name::<Self>());
        self.run().await.unwrap_or_else(|e| panic!("Failed to start MointoringEndpoint: {:?}", e));
    }
}

/// Returns prometheus metrics.
/// In case the node doesn’t collect metrics returns an empty response with status code 405: method
/// not allowed.
#[instrument(level = "debug", ret, skip(prometheus_handle))]
// TODO(tsabary): handle the Option setup.
async fn metrics(prometheus_handle: Option<PrometheusHandle>) -> Response {
    match prometheus_handle {
        Some(handle) => handle.render().into_response(),
        None => StatusCode::METHOD_NOT_ALLOWED.into_response(),
    }
}

// Returns Mempool snapshot
#[instrument(level = "debug", skip(mempool_client))]
async fn mempool_snapshot(
    mempool_client: Option<SharedMempoolClient>,
) -> Result<Json<MempoolSnapshot>, StatusCode> {
    match mempool_client {
        Some(client) => match client.get_mempool_snapshot().await {
            Ok(snapshot) => Ok(snapshot.into()),
            Err(err) => {
                error!("Failed to get mempool snapshot: {:?}", err);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        },
        None => Err(StatusCode::METHOD_NOT_ALLOWED),
    }
}

// Change the global log level
#[instrument(level = "debug")]
async fn change_global_log_level(Path(log_level): Path<String>) -> StatusCode {
    info!("Changing global log level to: {}", log_level);
    let log_level = match LevelFilter::from_str(&log_level) {
        Ok(level) => level,
        Err(_) => {
            error!("Invalid log level: {}", log_level);
            return StatusCode::BAD_REQUEST;
        }
    };
    change_tracing_level(log_level).await;
    StatusCode::OK
}
