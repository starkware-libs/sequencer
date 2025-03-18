use std::net::{SocketAddr, ToSocketAddrs};

use apollo_http_server::test_utils::HttpTestClient;
use apollo_monitoring_endpoint::test_utils::MonitoringClient;
use mempool_test_utils::starknet_api_test_utils::{AccountId, MultiAccountTransactionGenerator};
use papyrus_base_layer::ethereum_base_layer_contract::{L1ToL2MessageArgs, StarknetL1Contract};
use papyrus_base_layer::test_utils::{
    deploy_starknet_l1_contract,
    ethereum_base_layer_config_for_anvil,
};
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;
use tracing::info;
use url::Url;

use crate::integration_test_manager::nonce_to_usize;
use crate::monitoring_utils;
use crate::utils::{send_consensus_txs, TestScenario};

// TODO(Arni): Add the Starknet L1 contract handler.
pub struct SequencerSimulator {
    monitoring_client: MonitoringClient,
    http_client: HttpTestClient,
    starknet_l1_contract: StarknetL1Contract,
}

impl SequencerSimulator {
    pub async fn create(
        http_url: String,
        http_port: u16,
        monitoring_url: String,
        monitoring_port: u16,
        base_layer_url: String,
        base_layer_port: u16,
    ) -> Self {
        let monitoring_client =
            MonitoringClient::new(get_socket_addr(&monitoring_url, monitoring_port).unwrap());

        let http_client = HttpTestClient::new(get_socket_addr(&http_url, http_port).unwrap());

        let mut base_layer_config = ethereum_base_layer_config_for_anvil(Some(base_layer_port));
        base_layer_config.node_url =
            Url::parse(format!("{}:{}", base_layer_url, base_layer_port).as_str()).unwrap();
        let starknet_l1_contract = deploy_starknet_l1_contract(base_layer_config).await;

        Self { monitoring_client, http_client, starknet_l1_contract }
    }

    pub async fn assert_add_tx_success(&self, tx: RpcTransaction) -> TransactionHash {
        info!("Sending transaction: {:?}", tx);
        self.http_client.assert_add_tx_success(tx).await
    }

    pub async fn send_l1_handler_tx(
        &self,
        l1_to_l2_message_args: L1ToL2MessageArgs,
    ) -> TransactionHash {
        info!("Sending L1 handler: {:?}", l1_to_l2_message_args);
        self.starknet_l1_contract.send_message_to_l2(&l1_to_l2_message_args).await;
        TransactionHash::default() // Arbitrary.
    }

    pub async fn send_txs(
        &self,
        tx_generator: &mut MultiAccountTransactionGenerator,
        test_scenario: &impl TestScenario,
        sender_account: AccountId,
    ) {
        info!("Sending transactions");
        let send_rpc_tx_fn = &mut |tx| self.assert_add_tx_success(tx);
        let send_l1_handler_tx_fn =
            &mut |l1_to_l2_message_args| self.send_l1_handler_tx(l1_to_l2_message_args);
        let tx_hashes = send_consensus_txs(
            tx_generator,
            sender_account,
            test_scenario,
            send_rpc_tx_fn,
            send_l1_handler_tx_fn,
        )
        .await;
        assert_eq!(tx_hashes.len(), test_scenario.n_txs());
    }

    pub async fn await_txs_accepted(&self, sequencer_idx: usize, target_n_batched_txs: usize) {
        monitoring_utils::await_txs_accepted(
            &self.monitoring_client,
            sequencer_idx,
            target_n_batched_txs,
        )
        .await;
    }

    pub async fn verify_txs_accepted(
        &self,
        sequencer_idx: usize,
        tx_generator: &mut MultiAccountTransactionGenerator,
        sender_account: AccountId,
    ) {
        let account = tx_generator.account_with_id(sender_account);
        let expected_n_batched_account_txs = nonce_to_usize(account.get_nonce());
        let expected_n_batched_l1_handler_txs = tx_generator.n_l1_txs();
        let expected_n_batched_txs =
            expected_n_batched_account_txs + expected_n_batched_l1_handler_txs;
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
