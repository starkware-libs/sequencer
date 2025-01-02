use std::future::Future;
use std::net::SocketAddr;
use std::time::Duration;

use axum::http::StatusCode;
use axum::routing::post;
use axum::Router;
use blockifier::context::ChainInfo;
use blockifier::test_utils::contracts::FeatureContract;
use blockifier::test_utils::{CairoVersion, RunnableCairo1};
use mempool_test_utils::starknet_api_test_utils::{
    AccountId,
    AccountTransactionGenerator,
    Contract,
    MultiAccountTransactionGenerator,
};
use papyrus_consensus::config::{ConsensusConfig, TimeoutsConfig};
use papyrus_consensus::types::ValidatorId;
use papyrus_consensus_orchestrator::cende::RECORDER_WRITE_BLOB_PATH;
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
use starknet_api::transaction::fields::ContractAddressSalt;
use starknet_api::transaction::TransactionHash;
use starknet_batcher::block_builder::BlockBuilderConfig;
use starknet_batcher::config::BatcherConfig;
use starknet_consensus_manager::config::ConsensusManagerConfig;
use starknet_gateway::config::{
    GatewayConfig,
    StatefulTransactionValidatorConfig,
    StatelessTransactionValidatorConfig,
};
use starknet_http_server::test_utils::create_http_server_config;
use starknet_mempool_p2p::config::MempoolP2pConfig;
use starknet_monitoring_endpoint::config::MonitoringEndpointConfig;
use starknet_sequencer_infra::test_utils::AvailablePorts;
use starknet_sequencer_node::config::component_config::ComponentConfig;
use starknet_sequencer_node::config::config_utils::{
    EthereumBaseLayerConfigRequiredParams,
    RequiredParams,
};
use starknet_sequencer_node::config::node_config::SequencerNodeConfig;
use starknet_state_sync::config::StateSyncConfig;
use starknet_types_core::felt::Felt;
use url::Url;

pub const ACCOUNT_ID_0: AccountId = 0;
pub const ACCOUNT_ID_1: AccountId = 1;
pub const NEW_ACCOUNT_SALT: ContractAddressSalt = ContractAddressSalt(Felt::THREE);
pub const UNDEPLOYED_ACCOUNT_ID: AccountId = 2;
// Transactions per second sent to the gateway. This rate makes each block contain ~10 transactions
// with the set [TimeoutsConfig] .
pub const TPS: u64 = 2;

pub fn create_chain_info() -> ChainInfo {
    let mut chain_info = ChainInfo::create_for_testing();
    // Note that the chain_id affects hashes of transactions and blocks, therefore affecting the
    // test.
    chain_info.chain_id = papyrus_storage::test_utils::CHAIN_ID_FOR_TESTS.clone();
    chain_info
}

// TODO(Tsabary/Shahak/Yair/AlonH): this function needs a proper cleaning.
#[allow(clippy::too_many_arguments)]
pub async fn create_node_config(
    available_ports: &mut AvailablePorts,
    sequencer_index: usize,
    chain_info: ChainInfo,
    batcher_storage_config: StorageConfig,
    state_sync_config: StateSyncConfig,
    mut consensus_manager_config: ConsensusManagerConfig,
    mempool_p2p_config: MempoolP2pConfig,
    component_config: ComponentConfig,
) -> (SequencerNodeConfig, RequiredParams) {
    let validator_id = set_validator_id(&mut consensus_manager_config, sequencer_index);
    let recorder_url = consensus_manager_config.cende_config.recorder_url.clone();
    let fee_token_addresses = chain_info.fee_token_addresses.clone();
    let batcher_config = create_batcher_config(batcher_storage_config, chain_info.clone());
    let gateway_config = create_gateway_config(chain_info.clone()).await;
    let http_server_config =
        create_http_server_config(available_ports.get_next_local_host_socket());
    let monitoring_endpoint_config =
        MonitoringEndpointConfig { port: available_ports.get_next_port(), ..Default::default() };

    (
        SequencerNodeConfig {
            batcher_config,
            consensus_manager_config,
            gateway_config,
            http_server_config,
            mempool_p2p_config,
            monitoring_endpoint_config,
            state_sync_config,
            components: component_config,
            ..Default::default()
        },
        RequiredParams {
            chain_id: chain_info.chain_id,
            eth_fee_token_address: fee_token_addresses.eth_fee_token_address,
            strk_fee_token_address: fee_token_addresses.strk_fee_token_address,
            validator_id,
            recorder_url,
            base_layer_config: EthereumBaseLayerConfigRequiredParams {
                node_url: Url::parse("https://node_url").expect("Should be a valid URL"),
            },
        },
    )
}

// TODO(Nadin/Tsabary): refactor this function to separate the creation of network_configs and
// broadcast_channels broadcast_channels into two distinct functions.
pub fn create_consensus_manager_configs_and_channels(
    n_managers: usize,
    ports: Vec<u16>,
) -> (Vec<ConsensusManagerConfig>, BroadcastTopicChannels<StreamMessage<ProposalPart>>) {
    let (network_configs, broadcast_channels) =
        create_network_configs_connected_to_broadcast_channels(
            papyrus_network::gossipsub_impl::Topic::new(
                starknet_consensus_manager::consensus_manager::CONSENSUS_PROPOSALS_TOPIC,
            ),
            ports,
        );
    // TODO: Need to also add a channel for votes, in addition to the proposals channel.

    // TODO(Matan, Dan): set reasonable default timeouts.
    let mut timeouts = TimeoutsConfig::default();
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
            ..Default::default()
        })
        .collect();

    (consensus_manager_configs, broadcast_channels)
}

// Creates a local recorder server that always returns a success status.
pub fn spawn_success_recorder(port: u16) -> Url {
    // [127, 0, 0, 1] is the localhost IP address.
    let socket_addr = SocketAddr::from(([127, 0, 0, 1], port));
    tokio::spawn(async move {
        let router = Router::new()
            .route(RECORDER_WRITE_BLOB_PATH, post(move || async { StatusCode::OK.to_string() }));
        axum::Server::bind(&socket_addr).serve(router.into_make_service()).await.unwrap();
    });

    Url::parse(&format!("http://{}", socket_addr)).expect("Parsing recorder url fail")
}

pub fn create_mempool_p2p_configs(chain_id: ChainId, ports: Vec<u16>) -> Vec<MempoolP2pConfig> {
    create_connected_network_configs(ports)
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
        tx_generator.register_deployed_account(account);
    }
    // TODO(yair): This is a hack to fund the new account during the setup. Move the registration to
    // the test body once funding is supported.
    let new_account_id = tx_generator.register_undeployed_account(
        FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1(RunnableCairo1::Casm)),
        NEW_ACCOUNT_SALT,
    );
    assert_eq!(new_account_id, UNDEPLOYED_ACCOUNT_ID);
    tx_generator
}

pub fn create_txs_for_integration_test(
    tx_generator: &mut MultiAccountTransactionGenerator,
) -> Vec<RpcTransaction> {
    // Create RPC transactions.
    let account0_invoke_nonce1 =
        tx_generator.account_with_id_mut(ACCOUNT_ID_0).generate_invoke_with_tip(2);
    let account0_invoke_nonce2 =
        tx_generator.account_with_id_mut(ACCOUNT_ID_0).generate_invoke_with_tip(3);
    let account1_invoke_nonce1 =
        tx_generator.account_with_id_mut(ACCOUNT_ID_1).generate_invoke_with_tip(4);

    vec![account0_invoke_nonce1, account0_invoke_nonce2, account1_invoke_nonce1]
}

pub fn create_funding_txs(
    tx_generator: &mut MultiAccountTransactionGenerator,
) -> Vec<RpcTransaction> {
    // TODO(yair): Register the undeployed account here instead of in the test setup
    // once funding is implemented.
    let undeployed_account = tx_generator.account_with_id(UNDEPLOYED_ACCOUNT_ID).account;
    assert!(tx_generator.undeployed_accounts().contains(&undeployed_account));
    fund_new_account(tx_generator.account_with_id_mut(ACCOUNT_ID_0), &undeployed_account)
}

fn fund_new_account(
    funding_account: &mut AccountTransactionGenerator,
    receipient: &Contract,
) -> Vec<RpcTransaction> {
    let funding_tx = funding_account.generate_transfer(receipient);
    vec![funding_tx]
}

fn create_account_txs(
    mut tx_generator: MultiAccountTransactionGenerator,
    account_id: AccountId,
    n_txs: usize,
) -> Vec<RpcTransaction> {
    (0..n_txs)
        .map(|_| tx_generator.account_with_id_mut(account_id).generate_invoke_with_tip(1))
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
        tokio::time::sleep(Duration::from_millis(1000 / TPS)).await;
        tx_hashes.push(send_rpc_tx_fn(rpc_tx).await);
    }
    tx_hashes
}

// TODO(yair): Consolidate create_rpc_txs_fn and test_tx_hashes_fn into a single function.
/// Creates and runs the integration test scenario for the sequencer integration test. Returns a
/// list of transaction hashes, in the order they are expected to be in the mempool.
pub async fn run_integration_test_scenario<'a, Fut>(
    tx_generator: &mut MultiAccountTransactionGenerator,
    create_rpc_txs_fn: impl Fn(&mut MultiAccountTransactionGenerator) -> Vec<RpcTransaction>,
    send_rpc_tx_fn: &'a mut dyn FnMut(RpcTransaction) -> Fut,
    test_tx_hashes_fn: impl Fn(&[TransactionHash]) -> Vec<TransactionHash>,
) -> Vec<TransactionHash>
where
    Fut: Future<Output = TransactionHash> + 'a,
{
    let rpc_txs = create_rpc_txs_fn(tx_generator);
    let tx_hashes = send_rpc_txs(rpc_txs, send_rpc_tx_fn).await;
    test_tx_hashes_fn(&tx_hashes)
}

pub fn test_tx_hashes_for_integration_test(tx_hashes: &[TransactionHash]) -> Vec<TransactionHash> {
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

pub fn create_state_sync_configs(
    n_sequencers: usize,
    state_sync_storage_config: StorageConfig,
    available_ports: &mut AvailablePorts,
) -> Vec<StateSyncConfig> {
    let mut state_sync_configs = vec![];
    let network_configs =
        create_connected_network_configs(available_ports.get_next_ports(n_sequencers));
    for network_config in network_configs {
        let state_sync_config = StateSyncConfig {
            storage_config: state_sync_storage_config.clone(),
            network_config,
            ..Default::default()
        };
        state_sync_configs.push(state_sync_config);
    }
    state_sync_configs
}
