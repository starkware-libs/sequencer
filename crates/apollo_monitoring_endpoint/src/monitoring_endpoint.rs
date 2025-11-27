use std::net::SocketAddr;

use apollo_infra::component_definitions::ComponentStarter;
use apollo_infra::metrics::{initialize_metrics_recorder, MetricsConfig};
use apollo_infra::trace_util::{configure_tracing, get_log_directives, set_log_level};
use apollo_infra_utils::type_name::short_type_name;
use apollo_l1_provider_types::{L1ProviderSnapshot, SharedL1ProviderClient};
use apollo_mempool_types::communication::SharedMempoolClient;
use apollo_mempool_types::mempool_types::MempoolSnapshot;
use apollo_monitoring_endpoint_config::config::MonitoringEndpointConfig;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{async_trait, Json, Router, Server};
use hyper::Error;
use metrics_exporter_prometheus::PrometheusHandle;
use tracing::level_filters::LevelFilter;
use tracing::{error, info, instrument};

#[cfg(test)]
#[path = "monitoring_endpoint_test.rs"]
mod monitoring_endpoint_test;

pub(crate) const MONITORING_PREFIX: &str = "monitoring";
pub(crate) const ALIVE: &str = "alive";
pub(crate) const READY: &str = "ready";
pub(crate) const VERSION: &str = "nodeVersion";
pub(crate) const METRICS: &str = "metrics";
pub(crate) const MEMPOOL_SNAPSHOT: &str = "mempoolSnapshot";
pub(crate) const L1_PROVIDER_SNAPSHOT: &str = "l1ProviderSnapshot";
pub(crate) const SET_LOG_LEVEL: &str = "setLogLevel";
pub(crate) const LOG_LEVEL: &str = "logLevel";

pub struct MonitoringEndpoint {
    config: MonitoringEndpointConfig,
    version: &'static str,
    prometheus_handle: Option<PrometheusHandle>,
    mempool_client: Option<SharedMempoolClient>,
    l1_provider_client: Option<SharedL1ProviderClient>,
}

impl MonitoringEndpoint {
    pub fn new(
        config: MonitoringEndpointConfig,
        version: &'static str,
        prometheus_handle: Option<PrometheusHandle>,
        mempool_client: Option<SharedMempoolClient>,
        l1_provider_client: Option<SharedL1ProviderClient>,
    ) -> Self {
        MonitoringEndpoint {
            config,
            version,
            prometheus_handle,
            mempool_client,
            l1_provider_client,
        }
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
        let l1_provider_client = self.l1_provider_client.clone();

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
                format!("/{MONITORING_PREFIX}/{L1_PROVIDER_SNAPSHOT}").as_str(),
                get(move || get_l1_provider_snapshot(l1_provider_client)),
            )
            .route(
                format!("/{MONITORING_PREFIX}/{SET_LOG_LEVEL}/:crate/:level").as_str(),
                post(set_log_level_endpoint),
            )
            .route(
                format!("/{MONITORING_PREFIX}/{LOG_LEVEL}").as_str(),
                get(get_log_directives_endpoint),
            )
    }
}

pub fn create_monitoring_endpoint(
    config: MonitoringEndpointConfig,
    version: &'static str,
    metrics_config: MetricsConfig,
    mempool_client: Option<SharedMempoolClient>,
    l1_provider_client: Option<SharedL1ProviderClient>,
) -> MonitoringEndpoint {
    let prometheus_handle = initialize_metrics_recorder(metrics_config);
    MonitoringEndpoint::new(config, version, prometheus_handle, mempool_client, l1_provider_client)
}

#[async_trait]
impl ComponentStarter for MonitoringEndpoint {
    async fn start(&mut self) {
        info!("Starting component {}.", short_type_name::<Self>());
        self.run().await.unwrap_or_else(|e| panic!("Failed to start MointoringEndpoint: {e:?}"));
    }
}

/// Returns prometheus metrics.
/// In case the node doesn’t collect metrics returns an empty response with status code 405: method
/// not allowed.
#[instrument(level = "trace", ret, skip(prometheus_handle))]
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
        Some(client) => {
            // Wrap the mempool client interaction with a tokio::spawn as it is NOT cancel-safe.
            // Even if the current task is cancelled, e.g., when a request is dropped while still
            // being processed, the inner task will continue to run.
            let mempool_snapshot_result =
                tokio::spawn(async move { client.get_mempool_snapshot().await })
                    .await
                    .expect("Should be able to get mempool_snapshot result");

            match mempool_snapshot_result {
                Ok(snapshot) => Ok(snapshot.into()),
                Err(err) => {
                    error!("Failed to get mempool snapshot: {:?}", err);
                    Err(StatusCode::INTERNAL_SERVER_ERROR)
                }
            }
        }
        None => Err(StatusCode::METHOD_NOT_ALLOWED),
    }
}

// Returns L1 provider snapshot
#[instrument(level = "debug", skip(l1_provider_client))]
async fn get_l1_provider_snapshot(
    l1_provider_client: Option<SharedL1ProviderClient>,
) -> Result<Json<L1ProviderSnapshot>, StatusCode> {
    match l1_provider_client {
        Some(client) => {
            // Wrap the l1 client interaction with a tokio::spawn as it is NOT cancel-safe.
            // Even if the current task is cancelled, e.g., when a request is dropped while still
            // being processed, the inner task will continue to run.
            let l1_provider_snapshot_result =
                tokio::spawn(async move { client.get_l1_provider_snapshot().await })
                    .await
                    .expect("Should be able to get l1 provider result");

            match l1_provider_snapshot_result {
                Ok(snapshot) => Ok(snapshot.into()),
                Err(err) => {
                    error!("Failed to get L1 provider snapshot: {:?}", err);
                    Err(StatusCode::INTERNAL_SERVER_ERROR)
                }
            }
        }
        None => Err(StatusCode::METHOD_NOT_ALLOWED),
    }
}

async fn set_log_level_endpoint(
    Path((crate_name, level)): Path<(String, String)>,
) -> Result<StatusCode, StatusCode> {
    let level_filter = level.parse::<LevelFilter>().map_err(|_| StatusCode::BAD_REQUEST)?;
    let handle = configure_tracing().await;
    set_log_level(&handle, &crate_name, level_filter);
    Ok(StatusCode::OK)
}

async fn get_log_directives_endpoint() -> impl IntoResponse {
    let handle = configure_tracing().await;
    match get_log_directives(&handle) {
        Ok(directives) => (StatusCode::OK, directives).into_response(),
        Err(err) => {
            (StatusCode::INTERNAL_SERVER_ERROR, format!("failed to read log directives: {err}"))
                .into_response()
        }
    }
}
