use std::net::{IpAddr, SocketAddr};

use axum::body::Body;
use axum::http::Request;
use hyper::client::HttpConnector;
use hyper::Client;
use starknet_infra_utils::run_until::run_until;
use starknet_infra_utils::tracing::{CustomLogger, TraceLevel};
use tracing::info;

use crate::monitoring_endpoint::{ALIVE, MONITORING_PREFIX};

/// Client for querying 'alive' status of an http server.
pub struct IsAliveClient {
    socket: SocketAddr,
    client: Client<HttpConnector>,
}

impl IsAliveClient {
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
}

pub(crate) fn build_request(ip: &IpAddr, port: u16, method: &str) -> Request<Body> {
    Request::builder()
        .uri(format!("http://{ip}:{port}/{MONITORING_PREFIX}/{method}").as_str())
        .body(Body::empty())
        .unwrap()
}
