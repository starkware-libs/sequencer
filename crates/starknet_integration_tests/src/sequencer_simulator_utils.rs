use std::net::SocketAddr;

use mempool_test_utils::starknet_api_test_utils::{AccountId, MultiAccountTransactionGenerator};
use starknet_api::block::BlockNumber;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_http_server::config::HttpServerConfig;
use starknet_http_server::test_utils::HttpTestClient;
use starknet_monitoring_endpoint::config::MonitoringEndpointConfig;
use starknet_monitoring_endpoint::test_utils::MonitoringClient;
use starknet_sequencer_node::utils::load_and_validate_config;
use tracing::info;

use crate::monitoring_utils::await_execution;
use crate::utils::{send_account_txs, TestScenario};

pub struct SequencerSimulator {
    monitoring_client: MonitoringClient,
    http_client: HttpTestClient,
}

impl SequencerSimulator {
    pub fn new(args: Vec<String>) -> Self {
        let config = load_and_validate_config(args).expect("Failed to load and validate config");

        let MonitoringEndpointConfig { ip, port, .. } = config.monitoring_endpoint_config;
        let monitoring_client = MonitoringClient::new(SocketAddr::from((ip, port)));

        let HttpServerConfig { ip, port } = config.http_server_config;
        let http_client = HttpTestClient::new(SocketAddr::from((ip, port)));

        Self { monitoring_client, http_client }
    }

    pub async fn assert_add_tx_success(&self, tx: RpcTransaction) -> TransactionHash {
        info!("Sending transaction: {:?}", tx);
        self.http_client.assert_add_tx_success(tx).await
    }

    pub async fn send_txs(
        &self,
        tx_generator: &mut MultiAccountTransactionGenerator,
        test_scenario: &impl TestScenario,
        sender_account: AccountId,
    ) {
        info!("Sending transactions");
        let send_rpc_tx_fn = &mut |tx| self.assert_add_tx_success(tx);
        let tx_hashes =
            send_account_txs(tx_generator, sender_account, test_scenario, send_rpc_tx_fn).await;
        assert_eq!(tx_hashes.len(), test_scenario.n_txs());
    }

    pub async fn await_execution(&self, expected_block_number: BlockNumber) {
        await_execution(&self.monitoring_client, expected_block_number, 0, 0).await;
    }

    // TODO(Nadin): Implement this function.
    pub async fn verify_txs_accepted(&self) {
        unimplemented!();
    }
}
