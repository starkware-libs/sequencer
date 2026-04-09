use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use apollo_gateway_types::communication::MockGatewayClient;
use apollo_gateway_types::gateway_types::GatewayOutput;
use apollo_http_server_config::config::{
    HttpServerConfig,
    HttpServerDynamicConfig,
    DEFAULT_MAX_SIERRA_PROGRAM_SIZE,
};
use apollo_infra::component_client::LocalComponentReaderClient;
use apollo_infra_utils::test_utils::{AvailablePorts, TestIdentifier};
use apollo_node_config::node_config::NodeDynamicConfig;
use blockifier_test_utils::cairo_versions::CairoVersion;
use http::StatusCode;
use mempool_test_utils::starknet_api_test_utils::{
    declare_tx,
    deploy_account_tx,
    invoke_tx,
    invoke_tx_client_side_proving,
};
use reqwest::{Body, Client, Response};
use serde::Serialize;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::fields::{Proof, ProofFacts};
use starknet_api::transaction::TransactionHash;
use tokio::time::sleep;

use crate::deprecated_gateway_transaction::DeprecatedGatewayTransactionV3;
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

    // TODO(Yael): add a check for the response content, for all GatewayOutput types.
    pub async fn assert_add_tx_success(&self, tx: impl GatewayTransaction) -> TransactionHash {
        let response = self.add_tx(tx).await;
        let status_code = response.status();
        let text = response.text().await.unwrap();
        assert!(status_code.is_success(), "{status_code:?}, {text}");
        let response: GatewayOutput = serde_json::from_str(&text)
            .unwrap_or_else(|_| panic!("Gateway responded with: {text}"));
        response.transaction_hash()
    }

    pub async fn assert_add_tx_error(
        &self,
        rpc_tx: impl GatewayTransaction,
        expected_error_status: StatusCode,
    ) -> String {
        let response = self.add_tx(rpc_tx).await;
        assert_eq!(
            response.status(),
            expected_error_status,
            "Unexpected status code. Expected: {}, got: {}",
            expected_error_status,
            response.status()
        );
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

    pub async fn add_rpc_tx_with_zstd(&self, compressed_body: Vec<u8>) -> Response {
        self.client
            .post(format!("http://{}/gateway/add_rpc_transaction", self.socket))
            .header("content-type", "application/json")
            .header("content-encoding", "zstd")
            .body(Body::from(compressed_body))
            .send()
            .await
            .unwrap()
    }
}

pub fn create_http_server_config(socket: SocketAddr) -> HttpServerConfig {
    HttpServerConfig::new(socket.ip(), socket.port(), DEFAULT_MAX_SIERRA_PROGRAM_SIZE)
}

pub struct HttpClientServerSetupBuilder {
    http_server_config: HttpServerConfig,
    http_server_dynamic_config: HttpServerDynamicConfig,
    mock_gateway_client: Option<MockGatewayClient>,
}

impl HttpClientServerSetupBuilder {
    // The builder must be invoked with different port_index values to support concurrent execution,
    // otherwise leading to Address already in use (os error 98) errors.
    // The input port_index is used to construct the http_server_config.
    pub fn new(port_index: u16) -> Self {
        let ip = IpAddr::from(Ipv4Addr::LOCALHOST);
        println!("Using port index {}", port_index);
        let mut available_ports =
            AvailablePorts::new(TestIdentifier::HttpServerUnitTests.into(), port_index);
        let http_server_config = HttpServerConfig::new(
            ip,
            available_ports.get_next_port(),
            DEFAULT_MAX_SIERRA_PROGRAM_SIZE,
        );
        Self::new_from_http_server_config(http_server_config)
    }

    // If a custom http server config is needed, don't hesitate to make this method public.
    fn new_from_http_server_config(http_server_config: HttpServerConfig) -> Self {
        Self {
            http_server_config,
            http_server_dynamic_config: HttpServerDynamicConfig {
                accept_new_txs: true,
                ..Default::default()
            },
            mock_gateway_client: None,
        }
    }

    pub fn with_max_request_body_size(mut self, max_request_body_size: usize) -> Self {
        self.http_server_config.static_config.max_request_body_size = max_request_body_size;
        self
    }

    pub fn with_http_server_dynamic_config(mut self, config: HttpServerDynamicConfig) -> Self {
        self.http_server_dynamic_config = config;
        self
    }

    pub fn with_mock_gateway_client(mut self, mock_gateway_client: MockGatewayClient) -> Self {
        self.mock_gateway_client = Some(mock_gateway_client);
        self
    }

    // Creates a client for testing the http server functionality.
    pub async fn build(self) -> HttpTestClient {
        let node_dynamic_config = NodeDynamicConfig {
            http_server_dynamic_config: Some(self.http_server_dynamic_config),
            ..Default::default()
        };
        let (_, config_manager_reader_client) =
            LocalComponentReaderClient::new_with_initial_value(node_dynamic_config);
        let gateway_client = Arc::new(self.mock_gateway_client.unwrap_or_default());

        // Spawn an http server wrapped in a retry mechanism.
        let mut remaining_retries: u8 = 5;
        loop {
            // Create the server struct (consumed by the spawned task).
            let mut http_server = HttpServer::new(
                self.http_server_config.clone(),
                config_manager_reader_client.clone(),
                gateway_client.clone(),
            );
            // Spawn the server.
            let handle = tokio::spawn(async move { http_server.run().await });

            // Let it run for a few milliseconds to ensure it has successfully started.
            const SLEEP_DURATION: Duration = Duration::from_millis(10);
            sleep(SLEEP_DURATION).await;

            // Check if the server is still running, if so, continue. Otherwise, log and repeat.
            if !handle.is_finished() {
                break;
            } else {
                println!("Server handle: {handle:?}");
                remaining_retries -= 1;
                assert!(remaining_retries > 0, "Failed spawning test http server");
            }
        }

        let (ip, port) = self.http_server_config.ip_and_port();
        let add_tx_http_client = HttpTestClient::new(SocketAddr::from((ip, port)));

        // Ensure the server starts running.
        tokio::task::yield_now().await;

        add_tx_http_client
    }
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

// Used for tx json that doesn't serialize into a valid tx to test the error handling of
// unsupported tx versions.
#[derive(Clone, Serialize)]
pub struct TransactionSerialization(pub serde_json::Value);
impl GatewayTransaction for TransactionSerialization {
    fn endpoint(&self) -> &str {
        "add_transaction"
    }

    fn content_type(&self) -> &str {
        "application/text"
    }
}

pub fn rpc_invoke_tx() -> RpcTransaction {
    invoke_tx(CairoVersion::default())
}

pub fn rpc_invoke_tx_client_side_proving() -> RpcTransaction {
    invoke_tx_client_side_proving(
        CairoVersion::default(),
        ProofFacts::snos_proof_facts_for_testing(),
        Proof::proof_for_testing(),
    )
}

pub fn deprecated_gateway_invoke_tx() -> DeprecatedGatewayTransactionV3 {
    DeprecatedGatewayTransactionV3::from(rpc_invoke_tx())
}

pub fn deprecated_gateway_invoke_tx_client_side_proving() -> DeprecatedGatewayTransactionV3 {
    DeprecatedGatewayTransactionV3::from(rpc_invoke_tx_client_side_proving())
}

pub fn deprecated_gateway_deploy_account_tx() -> DeprecatedGatewayTransactionV3 {
    DeprecatedGatewayTransactionV3::from(deploy_account_tx())
}

pub fn deprecated_gateway_declare_tx() -> DeprecatedGatewayTransactionV3 {
    DeprecatedGatewayTransactionV3::from(declare_tx())
}
