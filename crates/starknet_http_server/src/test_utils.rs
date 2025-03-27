use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use blockifier_test_utils::cairo_versions::CairoVersion;
use mempool_test_utils::starknet_api_test_utils::invoke_tx;
use reqwest::{Client, Response};
use serde::Serialize;
use starknet_api::rpc_transaction::{RpcInvokeTransaction, RpcTransaction};
use starknet_api::transaction::TransactionHash;
use starknet_gateway_types::communication::MockGatewayClient;
use starknet_gateway_types::errors::GatewaySpecError;
use strum_macros::IntoStaticStr;

use crate::config::HttpServerConfig;
use crate::http_server::HttpServer;
use crate::rest_api_transaction::{RestInvokeTransactionV3, RestTransactionV3};

/// A test utility client for interacting with an http server.
pub struct HttpTestClient {
    socket: SocketAddr,
    client: Client,
    endpoint: HttpServerEndpoint,
}

#[derive(Clone, Copy, IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum HttpServerEndpoint {
    AddTx,
    AddRpcTx,
}

impl HttpTestClient {
    pub fn new(socket: SocketAddr, endpoint: HttpServerEndpoint) -> Self {
        let client = Client::new();
        Self { socket, client, endpoint }
    }

    pub async fn assert_add_tx_success(&self, rpc_tx: impl Serialize) -> TransactionHash {
        let response = self.add_tx(rpc_tx).await;
        assert!(response.status().is_success());
        let text = response.text().await.unwrap();
        serde_json::from_str(&text).unwrap_or_else(|_| panic!("Gateway responded with: {}", text))
    }

    // TODO(Tsabary): implement when usage eventually arises.
    pub async fn assert_add_tx_error(&self, _tx: RpcTransaction) -> GatewaySpecError {
        todo!()
    }

    // Prefer using assert_add_tx_success or other higher level methods of this client, to ensure
    // tests are boilerplate and implementation-detail free.
    pub async fn add_tx(&self, tx: impl Serialize) -> Response {
        self.add_tx_with_headers(tx, []).await
    }

    pub async fn add_tx_with_headers<I>(&self, tx: impl Serialize, header_members: I) -> Response
    where
        I: IntoIterator<Item = (&'static str, &'static str)>,
    {
        let endpoint: &str = self.endpoint.into();
        let mut request = self.client.post(format!("http://{}/{}", self.socket, endpoint));
        for (key, value) in header_members {
            request = request.header(key, value);
        }
        let content_type = match self.endpoint {
            HttpServerEndpoint::AddTx => "application/text",
            HttpServerEndpoint::AddRpcTx => "application/json",
        };
        request
            .header("content-type", content_type)
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
    endpoint: HttpServerEndpoint,
) -> HttpTestClient {
    // Create and run the server.
    let mut http_server =
        HttpServer::new(http_server_config.clone(), Arc::new(mock_gateway_client));
    tokio::spawn(async move { http_server.run().await });

    let HttpServerConfig { ip, port } = http_server_config;
    let add_tx_http_client = HttpTestClient::new(SocketAddr::from((ip, port)), endpoint);

    // Ensure the server starts running.
    tokio::task::yield_now().await;

    add_tx_http_client
}

pub fn rpc_tx() -> RpcTransaction {
    invoke_tx(CairoVersion::default())
}

pub fn rest_tx() -> RestTransactionV3 {
    let tx = invoke_tx(CairoVersion::default());
    if let RpcTransaction::Invoke(RpcInvokeTransaction::V3(invoke_tx)) = tx {
        RestTransactionV3::Invoke(RestInvokeTransactionV3::from(invoke_tx))
    } else {
        panic!("Expected invoke transaction")
    }
}
