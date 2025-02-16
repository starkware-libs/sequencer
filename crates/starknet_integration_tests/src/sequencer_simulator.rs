use std::env::args;
use std::net::SocketAddr;
use std::process::exit;

use mempool_test_utils::starknet_api_test_utils::{AccountId, MultiAccountTransactionGenerator};
use papyrus_config::validators::config_validate;
use papyrus_config::ConfigError;
use starknet_api::block::BlockNumber;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_http_server::config::HttpServerConfig;
use starknet_http_server::test_utils::HttpTestClient;
use starknet_monitoring_endpoint::config::MonitoringEndpointConfig;
use starknet_monitoring_endpoint::test_utils::MonitoringClient;
use starknet_sequencer_node::config::node_config::SequencerNodeConfig;
use tracing::{error, info};

use crate::monitoring_utils::await_execution_simulator;
use crate::utils::{
    create_chain_info,
    create_consensus_manager_configs_from_network_configs,
    create_mempool_p2p_configs,
    create_state_sync_configs,
    send_account_txs,
    TestScenario,
};

pub fn load_and_validate_config(args: Vec<String>) -> Result<SequencerNodeConfig, ConfigError> {
    let config = SequencerNodeConfig::load_and_process(args);
    if let Err(ConfigError::CommandInput(clap_err)) = &config {
        error!("Failed loading configuration: {}", clap_err);
        clap_err.exit();
    }
    info!("Finished loading configuration.");

    let config = config?;
    if let Err(error) = config_validate(&config) {
        error!("{}", error);
        exit(1);
    }
    info!("Finished validating configuration.");

    Ok(config)
}

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
        await_execution_simulator(&self.monitoring_client, expected_block_number).await;
    }
}
