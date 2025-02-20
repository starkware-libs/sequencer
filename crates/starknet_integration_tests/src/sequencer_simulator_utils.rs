use std::net::{SocketAddr, ToSocketAddrs};

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
use url::Url;

use crate::monitoring_utils;
use crate::sequencer_manager::nonce_to_usize;
use crate::utils::{send_account_txs, TestScenario};

pub struct SequencerSimulator {
    monitoring_client: MonitoringClient,
    http_client: HttpTestClient,
}

impl SequencerSimulator {
    pub fn new(config_file: String, url: String) -> Self {
        // Calls `load_and_validate_config` with a dummy program name as the first argument,
        // since the function expects a vector of command-line arguments and ignores the first
        // entry.
        let config = load_and_validate_config(vec![
            "sequencer_simulator".to_string(),
            "--config_file".to_string(),
            config_file,
        ])
        .expect("Failed to load and validate config");

        let MonitoringEndpointConfig { ip: _, port, .. } = config.monitoring_endpoint_config;
        let monitoring_client = MonitoringClient::new(get_socket_addr(&url, port).unwrap());

        let HttpServerConfig { ip: _, port } = config.http_server_config;
        let http_client = HttpTestClient::new(get_socket_addr(&url, port).unwrap());

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
        monitoring_utils::await_execution(&self.monitoring_client, expected_block_number, 0, 0)
            .await;
    }

    pub async fn verify_txs_accepted(
        &self,
        sequencer_idx: usize,
        tx_generator: &mut MultiAccountTransactionGenerator,
        sender_account: AccountId,
    ) {
        let account = tx_generator.account_with_id(sender_account);
        let expected_n_batched_txs = nonce_to_usize(account.get_nonce());
        info!(
            "Verifying that sequencer {} got {} batched txs.",
            sequencer_idx, expected_n_batched_txs
        );
        monitoring_utils::verify_txs_accepted(
            &self.monitoring_client,
            sequencer_idx,
            expected_n_batched_txs,
        )
        .await;
    }
}

fn get_socket_addr(url_str: &str, port: u16) -> Result<SocketAddr, Box<dyn std::error::Error>> {
    let url = Url::parse(url_str)?;
    let host = url.host_str().ok_or("Invalid URL: no host found")?;
    let addr = format!("{}:{}", host, port)
        .to_socket_addrs()?
        .next()
        .ok_or("Failed to resolve address")?;

    Ok(addr)
}
