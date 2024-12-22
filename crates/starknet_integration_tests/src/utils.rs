use std::future::Future;
use std::net::SocketAddr;
use std::time::Duration;

use blockifier::context::ChainInfo;
use blockifier::test_utils::contracts::FeatureContract;
use blockifier::test_utils::{CairoVersion, RunnableCairo1};
use mempool_test_utils::starknet_api_test_utils::{AccountId, MultiAccountTransactionGenerator};
use papyrus_consensus::config::ConsensusConfig;
use papyrus_consensus::types::ValidatorId;
use papyrus_network::network_manager::test_utils::{
    create_connected_network_configs,
    create_network_configs_connected_to_broadcast_channels,
};
use papyrus_network::network_manager::BroadcastTopicChannels;
use papyrus_protobuf::consensus::{ProposalPart, StreamMessage};
use papyrus_storage::StorageConfig;
use starknet_api::block::BlockNumber;
use starknet_api::core::ChainId;
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
use starknet_http_server::config::HttpServerConfig;
use starknet_http_server::test_utils::create_http_server_config;
use starknet_mempool_p2p::config::MempoolP2pConfig;
use starknet_monitoring_endpoint::config::MonitoringEndpointConfig;
use starknet_sequencer_infra::test_utils::{get_available_socket, AvailablePorts};
use starknet_sequencer_node::config::node_config::SequencerNodeConfig;
use starknet_sequencer_node::config::test_utils::RequiredParams;
use starknet_state_sync::config::StateSyncConfig;
use starknet_types_core::felt::Felt;

pub fn create_chain_info() -> ChainInfo {
    let mut chain_info = ChainInfo::create_for_testing();
    // Note that the chain_id affects hashes of transactions and blocks, therefore affecting the
    // test.
    chain_info.chain_id = papyrus_storage::test_utils::CHAIN_ID_FOR_TESTS.clone();
    chain_info
}

// TODO(Tsabary/Shahak/Yair/AlonH): this function needs a proper cleaning.
// TODO(yair, Tsabary): Create config presets for tests, then remove all the functions that modify
// the config.
#[allow(clippy::too_many_arguments)]
pub async fn create_config(
    available_ports: &mut AvailablePorts,
    sequencer_index: usize,
    chain_info: ChainInfo,
    rpc_server_addr: SocketAddr,
    batcher_storage_config: StorageConfig,
    state_sync_storage_config: StorageConfig,
    mut consensus_manager_config: ConsensusManagerConfig,
    mempool_p2p_config: MempoolP2pConfig,
) -> (SequencerNodeConfig, RequiredParams) {
    let validator_id = set_validator_id(&mut consensus_manager_config, sequencer_index);
    let fee_token_addresses = chain_info.fee_token_addresses.clone();
    let batcher_config = create_batcher_config(batcher_storage_config, chain_info.clone());
    let gateway_config = create_gateway_config(chain_info.clone()).await;
    let http_server_config =
        create_http_server_config(available_ports.get_next_local_host_socket()).await;
    let rpc_state_reader_config = test_rpc_state_reader_config(rpc_server_addr);
    let monitoring_endpoint_config =
        MonitoringEndpointConfig { port: available_ports.get_next_port(), ..Default::default() };
    let state_sync_config =
        create_state_sync_config(state_sync_storage_config, available_ports.get_next_port());

    (
        SequencerNodeConfig {
            batcher_config,
            consensus_manager_config,
            gateway_config,
            http_server_config,
            rpc_state_reader_config,
            mempool_p2p_config,
            monitoring_endpoint_config,
            state_sync_config,
            ..Default::default()
        },
        RequiredParams {
            chain_id: chain_info.chain_id,
            eth_fee_token_address: fee_token_addresses.eth_fee_token_address,
            strk_fee_token_address: fee_token_addresses.strk_fee_token_address,
            validator_id,
        },
    )
}

pub fn create_consensus_manager_configs_and_channels(
    n_managers: usize,
    available_ports: &mut AvailablePorts,
) -> (Vec<ConsensusManagerConfig>, BroadcastTopicChannels<StreamMessage<ProposalPart>>) {
    let (network_configs, broadcast_channels) =
        create_network_configs_connected_to_broadcast_channels(
            n_managers,
            papyrus_network::gossipsub_impl::Topic::new(
                starknet_consensus_manager::consensus_manager::CONSENSUS_PROPOSALS_TOPIC,
            ),
            available_ports,
        );
    // TODO: Need to also add a channel for votes, in addition to the proposals channel.

    // TODO(Matan, Dan): set reasonable default timeouts.
    let mut timeouts = papyrus_consensus::config::TimeoutsConfig::default();
    timeouts.precommit_timeout *= 3;
    timeouts.prevote_timeout *= 3;
    timeouts.proposal_timeout *= 3;

    let consensus_manager_configs = network_configs
        .into_iter()
        // TODO(Matan): Get config from default config file.
        .map(|network_config| ConsensusManagerConfig {
            consensus_config: ConsensusConfig {
                start_height: BlockNumber(1),
		// TODO(Matan, Dan): Set the right amount
                consensus_delay: Duration::from_secs(15),
                network_config,
                num_validators: u64::try_from(n_managers).unwrap(),
                timeouts: timeouts.clone(),
                ..Default::default()
            },
        })
        .collect();

    (consensus_manager_configs, broadcast_channels)
}

pub fn test_rpc_state_reader_config(rpc_server_addr: SocketAddr) -> RpcStateReaderConfig {
    // TODO(Tsabary): get the latest version from the RPC crate.
    const RPC_SPEC_VERSION: &str = "V0_8";
    RpcStateReaderConfig::from_url(format!("http://{rpc_server_addr:?}/rpc/{RPC_SPEC_VERSION}"))
}

pub fn create_mempool_p2p_configs(
    n_mempools: usize,
    chain_id: ChainId,
    available_ports: &mut AvailablePorts,
) -> Vec<MempoolP2pConfig> {
    create_connected_network_configs(n_mempools, available_ports)
        .into_iter()
        .map(|mut network_config| {
            network_config.chain_id = chain_id.clone();
            MempoolP2pConfig { network_config, ..Default::default() }
        })
        .collect()
}

/// Creates a multi-account transaction generator for integration tests.
pub fn create_integration_test_tx_generator() -> MultiAccountTransactionGenerator {
    let mut tx_generator: MultiAccountTransactionGenerator =
        MultiAccountTransactionGenerator::new();

    for account in [
        FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1(RunnableCairo1::Casm)),
        FeatureContract::AccountWithoutValidations(CairoVersion::Cairo0),
    ] {
        tx_generator.register_account_for_flow_test(account);
    }
    tx_generator
}

fn create_txs_for_integration_test(
    tx_generator: &mut MultiAccountTransactionGenerator,
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
    tx_generator: &mut MultiAccountTransactionGenerator,
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
    let rpc_txs = create_account_txs(tx_generator, account_id, n_txs);
    send_rpc_txs(rpc_txs, send_rpc_tx_fn).await
}

pub async fn create_gateway_config(chain_info: ChainInfo) -> GatewayConfig {
    let stateless_tx_validator_config = StatelessTransactionValidatorConfig {
        validate_non_zero_l1_gas_fee: true,
        max_calldata_length: 10,
        max_signature_length: 2,
        ..Default::default()
    };
    let stateful_tx_validator_config = StatefulTransactionValidatorConfig::default();

    GatewayConfig { stateless_tx_validator_config, stateful_tx_validator_config, chain_info }
}

// TODO(Tsabary): deprecate this function.
pub async fn create_http_server_config_to_be_deprecated() -> HttpServerConfig {
    // TODO(Tsabary): use ser_generated_param.
    let socket = get_available_socket().await;
    HttpServerConfig { ip: socket.ip(), port: socket.port() }
}

pub fn create_batcher_config(
    batcher_storage_config: StorageConfig,
    chain_info: ChainInfo,
) -> BatcherConfig {
    // TODO(Arni): Create BlockBuilderConfig create for testing method and use here.
    BatcherConfig {
        storage: batcher_storage_config,
        block_builder_config: BlockBuilderConfig { chain_info, ..Default::default() },
        ..Default::default()
    }
}

fn set_validator_id(
    consensus_manager_config: &mut ConsensusManagerConfig,
    sequencer_index: usize,
) -> ValidatorId {
    let validator_id = ValidatorId::try_from(
        Felt::from(consensus_manager_config.consensus_config.validator_id)
            + Felt::from(sequencer_index),
    )
    .unwrap();
    consensus_manager_config.consensus_config.validator_id = validator_id;
    validator_id
}

fn create_state_sync_config(
    state_sync_storage_config: StorageConfig,
    port: u16,
) -> StateSyncConfig {
    let mut config =
        StateSyncConfig { storage_config: state_sync_storage_config, ..Default::default() };
    config.network_config.tcp_port = port;
    config
}
