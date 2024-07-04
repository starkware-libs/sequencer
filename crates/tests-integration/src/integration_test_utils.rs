use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use reqwest::{Client, Response};
use starknet_api::rpc_transaction::RPCTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_gateway::config::{
    GatewayConfig, GatewayNetworkConfig, RpcStateReaderConfig, StatefulTransactionValidatorConfig,
    StatelessTransactionValidatorConfig,
};
use starknet_gateway::errors::GatewayError;
use starknet_gateway::gateway::Gateway;
use starknet_gateway::rpc_state_reader::RpcStateReaderFactory;
use starknet_mempool_types::communication::SharedMempoolClient;
use test_utils::starknet_api_test_utils::external_tx_to_json;

use crate::state_reader::spawn_test_rpc_state_reader;

pub async fn create_gateway(
    mempool_client: SharedMempoolClient,
    n_initialized_account_contracts: u16,
) -> Gateway {
    let stateless_tx_validator_config = StatelessTransactionValidatorConfig {
        validate_non_zero_l1_gas_fee: true,
        max_calldata_length: 10,
        max_signature_length: 2,
        ..Default::default()
    };

    let socket: SocketAddr = "127.0.0.1:3000".parse().unwrap();
    let network_config = GatewayNetworkConfig { ip: socket.ip(), port: socket.port() };
    let stateful_tx_validator_config = StatefulTransactionValidatorConfig::create_for_testing();

    let gateway_config = GatewayConfig {
        network_config,
        stateless_tx_validator_config,
        stateful_tx_validator_config,
    };

    let rpc_server_addr = spawn_test_rpc_state_reader(n_initialized_account_contracts).await;
    let rpc_state_reader_config = spawn_test_rpc_state_reader_config(rpc_server_addr);
    let state_reader_factory = Arc::new(RpcStateReaderFactory { config: rpc_state_reader_config });

    Gateway::new(gateway_config, state_reader_factory, mempool_client)
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

    pub async fn assert_add_tx_success(&self, tx: &RPCTransaction) -> TransactionHash {
        let response = self.add_tx(tx).await;
        assert!(response.status().is_success());

        response.json().await.unwrap()
    }

    // TODO: implement when usage eventually arises.
    pub async fn assert_add_tx_error(&self, _tx: &RPCTransaction) -> GatewayError {
        todo!()
    }

    // Prefer using assert_add_tx_success or other higher level methods of this client, to ensure
    // tests are boilerplate and implementation-detail free.
    pub async fn add_tx(&self, tx: &RPCTransaction) -> Response {
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

fn spawn_test_rpc_state_reader_config(rpc_server_addr: SocketAddr) -> RpcStateReaderConfig {
    const RPC_SPEC_VERION: &str = "V0_7";
    const JSON_RPC_VERSION: &str = "2.0";
    RpcStateReaderConfig {
        url: format!("http://{rpc_server_addr:?}/rpc/{RPC_SPEC_VERION}"),
        json_rpc_version: JSON_RPC_VERSION.to_string(),
    }
}
