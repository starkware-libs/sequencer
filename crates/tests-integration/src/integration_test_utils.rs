use std::net::SocketAddr;

use axum::body::Body;
use blockifier::context::ChainInfo;
use mempool_test_utils::starknet_api_test_utils::rpc_tx_to_json;
use papyrus_storage::StorageConfig;
use reqwest::{Client, Response};
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_batcher::config::BatcherConfig;
use starknet_gateway::config::{
    GatewayConfig,
    RpcStateReaderConfig,
    StatefulTransactionValidatorConfig,
    StatelessTransactionValidatorConfig,
};
use starknet_gateway_types::errors::GatewaySpecError;
use starknet_http_server::config::HttpServerConfig;
use starknet_mempool_node::config::SequencerNodeConfig;
use tokio::net::TcpListener;

pub async fn create_config(
    rpc_server_addr: SocketAddr,
    batcher_storage_config: StorageConfig,
) -> SequencerNodeConfig {
    let chain_id = batcher_storage_config.db_config.chain_id.clone();
    let batcher_config = create_batcher_config(batcher_storage_config);
    let gateway_config = create_gateway_config().await;
    let http_server_config = create_http_server_config().await;
    let rpc_state_reader_config = test_rpc_state_reader_config(rpc_server_addr);
    SequencerNodeConfig {
        chain_id,
        batcher_config,
        gateway_config,
        http_server_config,
        rpc_state_reader_config,
        ..SequencerNodeConfig::default()
    }
}

pub fn test_rpc_state_reader_config(rpc_server_addr: SocketAddr) -> RpcStateReaderConfig {
    const RPC_SPEC_VERION: &str = "V0_8";
    const JSON_RPC_VERSION: &str = "2.0";
    RpcStateReaderConfig {
        url: format!("http://{rpc_server_addr:?}/rpc/{RPC_SPEC_VERION}"),
        json_rpc_version: JSON_RPC_VERSION.to_string(),
    }
}

/// Returns a unique IP address and port for testing purposes.
///
/// Tests run in parallel, so servers (like RPC or web) running on separate tests must have
/// different ports, otherwise the server will fail with "address already in use".
pub async fn get_available_socket() -> SocketAddr {
    // Dynamically select port.
    // First, set the port to 0 (dynamic port).
    TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind to address")
        // Then, resolve to the actual selected port.
        .local_addr()
        .expect("Failed to get local address")
}

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

    pub async fn assert_add_tx_success(&self, tx: &RpcTransaction) -> TransactionHash {
        let response = self.add_tx(tx).await;
        assert!(response.status().is_success());

        response.json().await.unwrap()
    }

    // TODO: implement when usage eventually arises.
    pub async fn assert_add_tx_error(&self, _tx: &RpcTransaction) -> GatewaySpecError {
        todo!()
    }

    // Prefer using assert_add_tx_success or other higher level methods of this client, to ensure
    // tests are boilerplate and implementation-detail free.
    pub async fn add_tx(&self, tx: &RpcTransaction) -> Response {
        let tx_json = rpc_tx_to_json(tx);
        self.client
            .post(format!("http://{}/add_tx", self.socket))
            .header("content-type", "application/json")
            .body(Body::from(tx_json))
            .send()
            .await
            .unwrap()
    }
}

async fn create_gateway_config() -> GatewayConfig {
    let stateless_tx_validator_config = StatelessTransactionValidatorConfig {
        validate_non_zero_l1_gas_fee: true,
        max_calldata_length: 10,
        max_signature_length: 2,
        ..Default::default()
    };
    let stateful_tx_validator_config = StatefulTransactionValidatorConfig::default();
    let chain_info = ChainInfo::create_for_testing();

    GatewayConfig { stateless_tx_validator_config, stateful_tx_validator_config, chain_info }
}

async fn create_http_server_config() -> HttpServerConfig {
    let socket = get_available_socket().await;
    HttpServerConfig { ip: socket.ip(), port: socket.port() }
}

fn create_batcher_config(batcher_storage_config: StorageConfig) -> BatcherConfig {
    BatcherConfig { storage: batcher_storage_config, ..Default::default() }
}
