use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use axum::body::Body;
use axum::http::Request;
use hyper::client::HttpConnector;
use hyper::Client;
use tracing::info;

use crate::monitoring_endpoint::{ALIVE, MONITORING_PREFIX};

// TODO(Tsabary): Clean feature dependencies and dev dependencies.

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
            .map_or(false, |response| response.status().is_success())
    }

    /// Blocks until 'alive'.
    pub async fn await_alive(&self) {
        let mut counter = 0;
        while !(self.query_alive().await) {
            info!("Waiting for node to be alive: {}.", counter);
            tokio::time::sleep(Duration::from_secs(1)).await;
            counter += 1;
        }
    }
}

pub(crate) fn build_request(ip: &IpAddr, port: u16, method: &str) -> Request<Body> {
    Request::builder()
        .uri(format!("http://{ip}:{port}/{MONITORING_PREFIX}/{method}").as_str())
        .body(Body::empty())
        .unwrap()
}
