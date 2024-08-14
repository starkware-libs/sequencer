use std::net::SocketAddr;

use axum::body::Body;
use blockifier::test_utils::contracts::FeatureContract;
use mempool_test_utils::starknet_api_test_utils::{
    external_tx_to_json,
    MultiAccountTransactionGenerator,
};
use reqwest::{Client, Response};
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_gateway::config::{
    GatewayConfig,
    GatewayNetworkConfig,
    RpcStateReaderConfig,
    StatefulTransactionValidatorConfig,
    StatelessTransactionValidatorConfig,
};
use starknet_gateway::errors::GatewaySpecError;
use starknet_mempool_node::config::MempoolNodeConfig;
use tokio::net::TcpListener;

use crate::integration_test_setup::IntegrationTestSetup;

async fn create_gateway_config() -> GatewayConfig {
    let stateless_tx_validator_config = StatelessTransactionValidatorConfig {
        validate_non_zero_l1_gas_fee: true,
        max_calldata_length: 10,
        max_signature_length: 2,
        ..Default::default()
    };

    let socket = get_available_socket().await;
    let network_config = GatewayNetworkConfig { ip: socket.ip(), port: socket.port() };
    let stateful_tx_validator_config = StatefulTransactionValidatorConfig::create_for_testing();

    GatewayConfig { network_config, stateless_tx_validator_config, stateful_tx_validator_config }
}

pub async fn create_config(rpc_server_addr: SocketAddr) -> MempoolNodeConfig {
    let gateway_config = create_gateway_config().await;
    let rpc_state_reader_config = test_rpc_state_reader_config(rpc_server_addr);
    MempoolNodeConfig { gateway_config, rpc_state_reader_config, ..MempoolNodeConfig::default() }
}

/// A test utility client for interacting with a gateway server.
pub struct GatewayClient {
    socket: SocketAddr,
    client: Client,
}

impl GatewayClient {
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
        let tx_json = external_tx_to_json(tx);
        self.client
            .post(format!("http://{}/add_tx", self.socket))
            .header("content-type", "application/json")
            .body(Body::from(tx_json))
            .send()
            .await
            .unwrap()
    }
}

fn test_rpc_state_reader_config(rpc_server_addr: SocketAddr) -> RpcStateReaderConfig {
    const RPC_SPEC_VERION: &str = "V0_7";
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
    // Dinamically select port.
    // First, set the port to 0 (dynamic port).
    TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind to address")
        // Then, resolve to the actual selected port.
        .local_addr()
        .expect("Failed to get local address")
}

/// Use to create a tx generator with _pre-funded_ accounts, alongside a mocked test setup.
pub async fn setup_with_tx_generation(
    accounts: &[FeatureContract],
) -> (IntegrationTestSetup, MultiAccountTransactionGenerator) {
    let integration_test_setup =
        IntegrationTestSetup::new_for_account_contracts(accounts.iter().copied()).await;
    let tx_generator =
        MultiAccountTransactionGenerator::new_for_account_contracts(accounts.iter().copied());

    (integration_test_setup, tx_generator)
}
