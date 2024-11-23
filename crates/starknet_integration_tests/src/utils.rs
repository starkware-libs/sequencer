use std::future::Future;
use std::net::SocketAddr;
use std::time::Duration;

use axum::body::Body;
use blockifier::context::ChainInfo;
use blockifier::test_utils::contracts::FeatureContract;
use blockifier::test_utils::CairoVersion;
use mempool_test_utils::starknet_api_test_utils::{
    rpc_tx_to_json,
    AccountId,
    MultiAccountTransactionGenerator,
};
use papyrus_consensus::config::ConsensusConfig;
use papyrus_network::network_manager::test_utils::create_network_config_connected_to_broadcast_channels;
use papyrus_network::network_manager::BroadcastTopicChannels;
use papyrus_protobuf::consensus::ProposalPart;
use papyrus_storage::StorageConfig;
use reqwest::{Client, Response};
use starknet_api::block::BlockNumber;
use starknet_api::contract_address;
use starknet_api::core::ContractAddress;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_batcher::block_builder::BlockBuilderConfig;
use starknet_batcher::config::BatcherConfig;
use starknet_consensus_manager::config::ConsensusManagerConfig;
use starknet_gateway::config::{
    GatewayConfig,
    RpcStateReaderConfig,
    StatefulTransactionValidatorConfig,
    StatelessTransactionValidatorConfig,
};
use starknet_gateway_types::errors::GatewaySpecError;
use starknet_http_server::config::HttpServerConfig;
use starknet_sequencer_infra::test_utils::get_available_socket;
use starknet_sequencer_node::config::node_config::SequencerNodeConfig;
use starknet_sequencer_node::config::test_utils::RequiredParams;

<<<<<<< Updated upstream
pub fn create_chain_info() -> ChainInfo {
    let mut chain_info = ChainInfo::create_for_testing();
    // Note that the chain_id affects hashes of transactions and blocks, therefore affecting the
    // test.
    chain_info.chain_id = papyrus_storage::test_utils::CHAIN_ID_FOR_TESTS.clone();
    chain_info
}

pub async fn create_config(
    chain_info: ChainInfo,
    rpc_server_addr: SocketAddr,
    batcher_storage_config: StorageConfig,
) -> (SequencerNodeConfig, RequiredParams, BroadcastTopicChannels<ProposalPart>) {
    let fee_token_addresses = chain_info.fee_token_addresses.clone();
    let batcher_config = create_batcher_config(batcher_storage_config, chain_info.clone());
    let gateway_config = create_gateway_config(chain_info.clone()).await;
    let http_server_config = create_http_server_config().await;
    let rpc_state_reader_config = test_rpc_state_reader_config(rpc_server_addr);
    let (consensus_manager_config, consensus_proposals_channels) =
        create_consensus_manager_config_and_channels();
    (
        SequencerNodeConfig {
            batcher_config,
            consensus_manager_config,
            gateway_config,
            http_server_config,
            rpc_state_reader_config,
            ..SequencerNodeConfig::default()
        },
        RequiredParams {
            chain_id: chain_info.chain_id,
            eth_fee_token_address: fee_token_addresses.eth_fee_token_address,
            strk_fee_token_address: fee_token_addresses.strk_fee_token_address,
            sequencer_address: ContractAddress::from(1312_u128), // Arbitrary non-zero value.
        },
        consensus_proposals_channels,
    )
}

fn create_consensus_manager_config_and_channels()
-> (ConsensusManagerConfig, BroadcastTopicChannels<ProposalPart>) {
    let (network_config, broadcast_channels) =
        create_network_config_connected_to_broadcast_channels(
            papyrus_network::gossipsub_impl::Topic::new(
                starknet_consensus_manager::consensus_manager::NETWORK_TOPIC,
            ),
        );
    let consensus_manager_config = ConsensusManagerConfig {
        consensus_config: ConsensusConfig {
            start_height: BlockNumber(1),
            consensus_delay: Duration::from_secs(1),
            network_config,
            ..Default::default()
        },
    };
    (consensus_manager_config, broadcast_channels)
}

pub fn test_rpc_state_reader_config(rpc_server_addr: SocketAddr) -> RpcStateReaderConfig {
    // TODO(Tsabary): get the latest version from the RPC crate.
    const RPC_SPEC_VERSION: &str = "V0_8";
    const JSON_RPC_VERSION: &str = "2.0";
    RpcStateReaderConfig {
        url: format!("http://{rpc_server_addr:?}/rpc/{RPC_SPEC_VERSION}"),
        json_rpc_version: JSON_RPC_VERSION.to_string(),
    }
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

    pub async fn assert_add_tx_success(&self, rpc_tx: RpcTransaction) -> TransactionHash {
        let response = self.add_tx(rpc_tx).await;
        assert!(response.status().is_success());

        response.json().await.unwrap()
    }

    // TODO: implement when usage eventually arises.
    pub async fn assert_add_tx_error(&self, _tx: RpcTransaction) -> GatewaySpecError {
        todo!()
    }

    // Prefer using assert_add_tx_success or other higher level methods of this client, to ensure
    // tests are boilerplate and implementation-detail free.
    pub async fn add_tx(&self, rpc_tx: RpcTransaction) -> Response {
        let tx_json = rpc_tx_to_json(&rpc_tx);
        self.client
            .post(format!("http://{}/add_tx", self.socket))
            .header("content-type", "application/json")
            .body(Body::from(tx_json))
            .send()
            .await
            .unwrap()
    }
}

/// Creates a multi-account transaction generator for integration tests.
pub fn create_integration_test_tx_generator() -> MultiAccountTransactionGenerator {
    let mut tx_generator: MultiAccountTransactionGenerator =
        MultiAccountTransactionGenerator::new();

    for account in [
        FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1),
        FeatureContract::AccountWithoutValidations(CairoVersion::Cairo0),
    ] {
        tx_generator.register_account_for_flow_test(account);
    }
    tx_generator
}

fn create_txs_for_integration_test(
    mut tx_generator: MultiAccountTransactionGenerator,
) -> Vec<RpcTransaction> {
    const ACCOUNT_ID_0: AccountId = 0;
    const ACCOUNT_ID_1: AccountId = 1;

    // Create RPC transactions.
    let account0_invoke_nonce1 =
        tx_generator.account_with_id(ACCOUNT_ID_0).generate_invoke_with_tip(2);
    let account0_invoke_nonce2 =
        tx_generator.account_with_id(ACCOUNT_ID_0).generate_invoke_with_tip(3);
    let account1_invoke_nonce1 =
        tx_generator.account_with_id(ACCOUNT_ID_1).generate_invoke_with_tip(4);

    vec![account0_invoke_nonce1, account0_invoke_nonce2, account1_invoke_nonce1]
}

fn create_account_txs(
    mut tx_generator: MultiAccountTransactionGenerator,
    account_id: AccountId,
    n_txs: usize,
) -> Vec<RpcTransaction> {
    (0..n_txs)
        .map(|_| tx_generator.account_with_id(account_id).generate_invoke_with_tip(1))
        .collect()
}

async fn send_rpc_txs<'a, Fut>(
    rpc_txs: Vec<RpcTransaction>,
    send_rpc_tx_fn: &'a mut dyn FnMut(RpcTransaction) -> Fut,
) -> Vec<TransactionHash>
where
    Fut: Future<Output = TransactionHash> + 'a,
{
    let mut tx_hashes = vec![];
    for rpc_tx in rpc_txs {
        tx_hashes.push(send_rpc_tx_fn(rpc_tx).await);
    }
    tx_hashes
}

/// Creates and runs the integration test scenario for the sequencer integration test. Returns a
/// list of transaction hashes, in the order they are expected to be in the mempool.
pub async fn run_integration_test_scenario<'a, Fut>(
    tx_generator: MultiAccountTransactionGenerator,
    send_rpc_tx_fn: &'a mut dyn FnMut(RpcTransaction) -> Fut,
) -> Vec<TransactionHash>
where
    Fut: Future<Output = TransactionHash> + 'a,
{
    let rpc_txs = create_txs_for_integration_test(tx_generator);
    let tx_hashes = send_rpc_txs(rpc_txs, send_rpc_tx_fn).await;

    // Return the transaction hashes in the order they should be given by the mempool:
    // Transactions from the same account are ordered by nonce; otherwise, higher tips are given
    // priority.
    assert!(
        tx_hashes.len() == 3,
        "Unexpected number of transactions sent in the integration test scenario. Found {} \
         transactions",
        tx_hashes.len()
    );
    vec![tx_hashes[2], tx_hashes[0], tx_hashes[1]]
}

/// Returns a list of the transaction hashes, in the order they are expected to be in the mempool.
pub async fn send_account_txs<'a, Fut>(
    tx_generator: MultiAccountTransactionGenerator,
    account_id: AccountId,
    n_txs: usize,
    send_rpc_tx_fn: &'a mut dyn FnMut(RpcTransaction) -> Fut,
) -> Vec<TransactionHash>
where
    Fut: Future<Output = TransactionHash> + 'a,
{
    let rpc_txs = create_account_txs(tx_generator, n_txs, account_id);
    send_rpc_txs(rpc_txs, send_rpc_tx_fn).await
}
