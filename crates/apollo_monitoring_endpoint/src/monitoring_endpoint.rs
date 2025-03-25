use std::net::SocketAddr;

use apollo_infra_utils::type_name::short_type_name;
use apollo_sequencer_infra::component_definitions::ComponentStarter;
use apollo_sequencer_metrics::metrics::COLLECT_SEQUENCER_PROFILING_METRICS;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{async_trait, Router, Server};
use hyper::Error;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use tracing::{info, instrument};

use crate::config::MonitoringEndpointConfig;

#[cfg(test)]
#[path = "monitoring_endpoint_test.rs"]
mod monitoring_endpoint_test;

pub(crate) const MONITORING_PREFIX: &str = "monitoring";
pub(crate) const ALIVE: &str = "alive";
pub(crate) const READY: &str = "ready";
pub(crate) const VERSION: &str = "nodeVersion";
pub(crate) const METRICS: &str = "metrics";

pub struct MonitoringEndpoint {
    config: MonitoringEndpointConfig,
    version: &'static str,
    prometheus_handle: Option<PrometheusHandle>,
}

impl MonitoringEndpoint {
    pub fn new(config: MonitoringEndpointConfig, version: &'static str) -> Self {
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
        MonitoringEndpoint { config, version, prometheus_handle }
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
    }
}

pub fn create_monitoring_endpoint(
    config: MonitoringEndpointConfig,
    version: &'static str,
) -> MonitoringEndpoint {
    MonitoringEndpoint::new(config, version)
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
