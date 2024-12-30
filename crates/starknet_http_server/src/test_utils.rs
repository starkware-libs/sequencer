use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use reqwest::{Client, Response};
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::test_utils::rpc_tx_to_json;
use starknet_api::transaction::TransactionHash;
use starknet_gateway_types::communication::MockGatewayClient;
use starknet_gateway_types::errors::GatewaySpecError;

use crate::config::HttpServerConfig;
use crate::http_server::HttpServer;

/// A test utility client for interacting with an http server.
pub struct HttpTestClient {
    socket: SocketAddr,
    client: Client,
}

impl HttpTestClient {
    pub fn new(socket: SocketAddr) -> Self {
        let client = Client::new();
        Self { socket, client }
    }

    pub async fn assert_add_tx_success(&self, rpc_tx: RpcTransaction) -> TransactionHash {
        let response = self.add_tx(rpc_tx).await;
        assert!(response.status().is_success());

        response.json().await.unwrap()
    }

    // TODO: implement when usage eventually arises.
    pub async fn assert_add_tx_error(&self, _tx: RpcTransaction) -> GatewaySpecError {
        todo!()
    }

    // Prefer using assert_add_tx_success or other higher level methods of this client, to ensure
    // tests are boilerplate and implementation-detail free.
    pub async fn add_tx(&self, rpc_tx: RpcTransaction) -> Response {
        self.add_tx_with_headers(rpc_tx, []).await
    }

    pub async fn add_tx_with_headers<I>(
        &self,
        rpc_tx: RpcTransaction,
        header_members: I,
    ) -> Response
    where
        I: IntoIterator<Item = (&'static str, &'static str)>,
    {
        let tx_json = rpc_tx_to_json(&rpc_tx);
        let mut request = self.client.post(format!("http://{}/add_tx", self.socket));
        for (key, value) in header_members {
            request = request.header(key, value);
        }
        request
            .header("content-type", "application/json")
            .body(Body::from(tx_json))
            .send()
            .await
            .unwrap()
    }
}

pub fn create_http_server_config(socket: SocketAddr) -> HttpServerConfig {
    HttpServerConfig { ip: socket.ip(), port: socket.port() }
}

/// Creates an HTTP server and an HttpTestClient that can interact with it.
pub async fn http_client_server_setup(
    mock_gateway_client: MockGatewayClient,
    ip: std::net::IpAddr,
) -> HttpTestClient {
    // TODO(Tsabary): replace the const port with something that is not hardcoded.
    // Create and run the server.
    let http_server_config = HttpServerConfig { ip, port: 15123 };
    let mut http_server =
        HttpServer::new(http_server_config.clone(), Arc::new(mock_gateway_client));
    tokio::spawn(async move { http_server.run().await });

    let HttpServerConfig { ip, port } = http_server_config;
    let add_tx_http_client = HttpTestClient::new(SocketAddr::from((ip, port)));

    // Ensure the server starts running.
    tokio::task::yield_now().await;

    add_tx_http_client
}
