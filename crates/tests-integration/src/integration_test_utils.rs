use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use reqwest::{Client, Response};
use starknet_api::rpc_transaction::RPCTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_gateway::config::{
    GatewayConfig, GatewayNetworkConfig, StatefulTransactionValidatorConfig,
    StatelessTransactionValidatorConfig,
};
use starknet_gateway::errors::GatewayError;
use starknet_gateway::gateway::Gateway;
use starknet_gateway::starknet_api_test_utils::external_tx_to_json;
use starknet_mempool_types::communication::SharedMempoolClient;

use crate::state_reader::rpc_test_state_reader_factory;

pub async fn create_gateway(mempool_client: SharedMempoolClient) -> Gateway {
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

    let state_reader_factory = Arc::new(rpc_test_state_reader_factory().await);

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
