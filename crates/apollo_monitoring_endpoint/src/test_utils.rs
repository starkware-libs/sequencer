use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;

use anyhow::Result;
use apollo_infra_utils::run_until::run_until;
use apollo_infra_utils::tracing::{CustomLogger, TraceLevel};
use apollo_metrics::metrics::parse_numeric_metric;
use axum::body::Body;
use axum::http::Request;
use hyper::body::to_bytes;
use hyper::client::HttpConnector;
use hyper::Client;
use num_traits::Num;
use thiserror::Error;
use tracing::info;

use crate::monitoring_endpoint::{ALIVE, METRICS, MONITORING_PREFIX};

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum MonitoringClientError {
    #[error("Failed to connect, error details: {}", connection_error)]
    ConnectionError { connection_error: String },
    #[error("Erroneous status: {}", status)]
    ResponseStatusError { status: String },
    #[error("Missing metric name: {}", metric_name)]
    MetricNotFound { metric_name: String },
}

/// Client for querying 'alive' status of an http server.
pub struct MonitoringClient {
    socket: SocketAddr,
    client: Client<HttpConnector>,
}

impl MonitoringClient {
    pub fn new(socket: SocketAddr) -> Self {
        let client = Client::new();
        Self { socket, client }
    }

    /// Returns 'true' if the server is 'alive'.
    async fn query_alive(&self) -> bool {
        info!("Querying the node for aliveness.");

        self.client
            .request(build_request(&self.socket.ip(), self.socket.port(), ALIVE))
            .await
            .is_ok_and(|response| response.status().is_success())
    }

    /// Blocks until 'alive', up to a maximum number of query attempts. Returns 'Ok(())' if the
    /// target is alive, otherwise 'Err(())'.
    pub async fn await_alive(&self, interval: u64, max_attempts: usize) -> Result<(), ()> {
        let condition = |node_is_alive: &bool| *node_is_alive;
        let query_alive_closure = || async move { self.query_alive().await };

        let logger =
            CustomLogger::new(TraceLevel::Info, Some("Waiting for node to be alive".to_string()));

        run_until(interval, max_attempts, query_alive_closure, condition, Some(logger))
            .await
            .ok_or(())
            .map(|_| ())
    }

    pub async fn get_metrics(&self) -> Result<String, MonitoringClientError> {
        let interval_ms = 200;
        let max_attempts = 25;

        let logger =
            CustomLogger::new(TraceLevel::Info, Some("Polling metrics endpoint".to_string()));

        let result = run_until(
            interval_ms,
            max_attempts,
            || async {
                self.client
                    .request(build_request(&self.socket.ip(), self.socket.port(), METRICS))
                    .await
            },
            |response_result| {
                response_result.as_ref().map(|res| res.status().is_success()).unwrap_or(false)
            },
            Some(logger),
        )
        .await;

        match result {
            Some(Ok(response)) => {
                let body_bytes = to_bytes(response.into_body()).await.unwrap();
                Ok(String::from_utf8(body_bytes.to_vec()).unwrap())
            }
            Some(Err(err)) => {
                Err(MonitoringClientError::ConnectionError { connection_error: err.to_string() })
            }
            None => Err(MonitoringClientError::ResponseStatusError {
                status: "Timeout or condition not met".into(),
            }),
        }
    }

    // TODO(Yael/Itay): add labels support
    // TODO(Itay): Consider making this private
    pub async fn get_metric<T: Num + FromStr>(
        &self,
        metric_name: &str,
    ) -> Result<T, MonitoringClientError> {
        let body_string = self.get_metrics().await?;

        // Extract and return the metric value, or a suitable error.
        parse_numeric_metric::<T>(&body_string, metric_name, None)
            .ok_or(MonitoringClientError::MetricNotFound { metric_name: metric_name.to_string() })
    }
}

pub(crate) fn build_request(ip: &IpAddr, port: u16, method: &str) -> Request<Body> {
    Request::builder()
        .uri(format!("http://{ip}:{port}/{MONITORING_PREFIX}/{method}").as_str())
        .body(Body::empty())
        .unwrap()
}
