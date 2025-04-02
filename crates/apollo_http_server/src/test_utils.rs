use std::net::SocketAddr;
use std::sync::Arc;

use apollo_gateway_types::communication::MockGatewayClient;
use apollo_gateway_types::gateway_types::GatewayOutput;
use axum::body::Body;
use blockifier_test_utils::cairo_versions::CairoVersion;
use hyper::StatusCode;
use mempool_test_utils::starknet_api_test_utils::invoke_tx;
use reqwest::{Client, Response};
use serde::Serialize;
use starknet_api::rpc_transaction::{RpcInvokeTransaction, RpcTransaction};
use starknet_api::transaction::TransactionHash;

use crate::config::HttpServerConfig;
use crate::deprecated_gateway_transaction::{
    DeprecatedGatewayInvokeTransaction,
    DeprecatedGatewayTransactionV3,
};
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

    pub async fn assert_add_tx_success(&self, tx: impl GatewayTransaction) -> TransactionHash {
        let response = self.add_tx(tx).await;
        assert!(response.status().is_success());
        let text = response.text().await.unwrap();
        let response: GatewayOutput = serde_json::from_str(&text)
            .unwrap_or_else(|_| panic!("Gateway responded with: {}", text));
        response.transaction_hash()
    }

    pub async fn assert_add_tx_error(
        &self,
        rpc_tx: impl GatewayTransaction,
        expected_error_status: StatusCode,
    ) -> String {
        let response = self.add_tx(rpc_tx).await;
        assert_eq!(response.status(), expected_error_status);
        response.text().await.unwrap()
    }

    // Prefer using assert_add_tx_success or other higher level methods of this client, to ensure
    // tests are boilerplate and implementation-detail free.
    pub async fn add_tx(&self, tx: impl GatewayTransaction) -> Response {
        self.add_tx_with_headers(tx, []).await
    }

    pub async fn add_tx_with_headers<I>(
        &self,
        tx: impl GatewayTransaction,
        header_members: I,
    ) -> Response
    where
        I: IntoIterator<Item = (&'static str, &'static str)>,
    {
        let mut request =
            self.client.post(format!("http://{}/gateway/{}", self.socket, tx.endpoint()));
        for (key, value) in header_members {
            request = request.header(key, value);
        }
        request
            .header("content-type", tx.content_type())
            .body(Body::from(serde_json::to_string(&tx).unwrap()))
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
    http_server_config: HttpServerConfig,
) -> HttpTestClient {
    // Create and run the server.
    let mut http_server =
        HttpServer::new(http_server_config.clone(), Arc::new(mock_gateway_client));
    tokio::spawn(async move { http_server.run().await });

    let HttpServerConfig { ip, port } = http_server_config;
    let add_tx_http_client = HttpTestClient::new(SocketAddr::from((ip, port)));

    // Ensure the server starts running.
    tokio::task::yield_now().await;

    add_tx_http_client
}

pub trait GatewayTransaction: Serialize + Clone {
    fn endpoint(&self) -> &str;
    fn content_type(&self) -> &str;
}

impl GatewayTransaction for RpcTransaction {
    fn endpoint(&self) -> &str {
        "add_rpc_transaction"
    }

    fn content_type(&self) -> &str {
        "application/json"
    }
}

impl GatewayTransaction for DeprecatedGatewayTransactionV3 {
    fn endpoint(&self) -> &str {
        "add_transaction"
    }

    fn content_type(&self) -> &str {
        "application/text"
    }
}

pub fn rpc_tx() -> RpcTransaction {
    invoke_tx(CairoVersion::default())
}

pub fn deprecated_gateway_tx() -> DeprecatedGatewayTransactionV3 {
    let tx = invoke_tx(CairoVersion::default());
    if let RpcTransaction::Invoke(RpcInvokeTransaction::V3(invoke_tx)) = tx {
        DeprecatedGatewayTransactionV3::Invoke(DeprecatedGatewayInvokeTransaction::V3(
            invoke_tx.into(),
        ))
    } else {
        panic!("Expected invoke transaction")
    }
}
