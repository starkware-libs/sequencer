use std::net::SocketAddr;

use apollo_infra::component_definitions::ComponentStarter;
use apollo_infra_utils::type_name::short_type_name;
use apollo_l1_provider_types::{L1ProviderSnapshot, SharedL1ProviderClient};
use apollo_mempool_types::communication::SharedMempoolClient;
use apollo_mempool_types::mempool_types::MempoolSnapshot;
use apollo_metrics::metrics::COLLECT_SEQUENCER_PROFILING_METRICS;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{async_trait, Json, Router, Server};
use hyper::Error;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
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
pub(crate) const L1_PROVIDER_SNAPSHOT: &str = "l1ProviderSnapshot";

const HISTOGRAM_BUCKETS: &[f64] =
    &[0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 25.0, 50.0];

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
        mempool_client: Option<SharedMempoolClient>,
        l1_provider_client: Option<SharedL1ProviderClient>,
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
                    .set_buckets(HISTOGRAM_BUCKETS)
                    .expect("Should be able to set buckets")
                    .install_recorder()
                    .expect("should be able to build the recorder and install it globally"),
            )
        } else {
            None
        };
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
    }
}

pub fn create_monitoring_endpoint(
    config: MonitoringEndpointConfig,
    version: &'static str,
    mempool_client: Option<SharedMempoolClient>,
    l1_provider_client: Option<SharedL1ProviderClient>,
) -> MonitoringEndpoint {
    MonitoringEndpoint::new(config, version, mempool_client, l1_provider_client)
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

// Returns L1 provider snapshot
#[instrument(level = "debug", skip(l1_provider_client))]
async fn get_l1_provider_snapshot(
    l1_provider_client: Option<SharedL1ProviderClient>,
) -> Result<Json<L1ProviderSnapshot>, StatusCode> {
    match l1_provider_client {
        Some(client) => match client.get_l1_provider_snapshot().await {
            Ok(snapshot) => Ok(snapshot.into()),
            Err(err) => {
                error!("Failed to get L1 provider snapshot: {:?}", err);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        },
        None => Err(StatusCode::METHOD_NOT_ALLOWED),
    }
}
