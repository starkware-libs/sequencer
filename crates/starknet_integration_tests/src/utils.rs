use std::future::Future;
use std::net::SocketAddr;
use std::time::Duration;

use alloy::primitives::U256;
use axum::http::StatusCode;
use axum::routing::post;
use axum::Router;
use blockifier::blockifier::config::TransactionExecutorConfig;
use blockifier::bouncer::{BouncerConfig, BouncerWeights};
use blockifier::context::ChainInfo;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::FeatureContract;
use futures::future::join_all;
use mempool_test_utils::starknet_api_test_utils::{
    AccountId,
    AccountTransactionGenerator,
    Contract,
    MultiAccountTransactionGenerator,
};
use papyrus_base_layer::ethereum_base_layer_contract::EthereumBaseLayerConfig;
use papyrus_base_layer::test_utils::StarknetL1Contract;
use papyrus_network::network_manager::test_utils::create_connected_network_configs;
use papyrus_network::NetworkConfig;
use papyrus_storage::StorageConfig;
use serde_json::to_value;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ChainId, ContractAddress};
use starknet_api::execution_resources::GasAmount;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::fields::ContractAddressSalt;
use starknet_api::transaction::{L1HandlerTransaction, TransactionHash, TransactionHasher};
use starknet_batcher::block_builder::BlockBuilderConfig;
use starknet_batcher::config::BatcherConfig;
use starknet_class_manager::class_storage::CachedClassStorageConfig;
use starknet_class_manager::config::{
    ClassManagerConfig,
    FsClassManagerConfig,
    FsClassStorageConfig,
};
use starknet_consensus::config::{ConsensusConfig, TimeoutsConfig};
use starknet_consensus::types::ValidatorId;
use starknet_consensus_manager::config::ConsensusManagerConfig;
use starknet_consensus_orchestrator::cende::{CendeConfig, RECORDER_WRITE_BLOB_PATH};
use starknet_consensus_orchestrator::config::ContextConfig;
use starknet_gateway::config::{
    GatewayConfig,
    StatefulTransactionValidatorConfig,
    StatelessTransactionValidatorConfig,
};
use starknet_http_server::test_utils::create_http_server_config;
use starknet_infra_utils::test_utils::AvailablePorts;
use starknet_l1_gas_price::eth_to_strk_oracle::EthToStrkOracleConfig;
use starknet_l1_provider::l1_scraper::L1ScraperConfig;
use starknet_l1_provider::L1ProviderConfig;
use starknet_mempool::config::MempoolConfig;
use starknet_mempool_p2p::config::MempoolP2pConfig;
use starknet_monitoring_endpoint::config::MonitoringEndpointConfig;
use starknet_sequencer_node::config::component_config::ComponentConfig;
use starknet_sequencer_node::config::definitions::ConfigPointersMap;
use starknet_sequencer_node::config::node_config::{SequencerNodeConfig, CONFIG_POINTERS};
use starknet_state_sync::config::StateSyncConfig;
use starknet_types_core::felt::Felt;
use tokio::task::JoinHandle;
use tracing::{debug, info, Instrument};
use url::Url;

use crate::executable_setup::NodeExecutionId;

pub const ACCOUNT_ID_0: AccountId = 0;
pub const ACCOUNT_ID_1: AccountId = 1;
pub const NEW_ACCOUNT_SALT: ContractAddressSalt = ContractAddressSalt(Felt::THREE);
pub const UNDEPLOYED_ACCOUNT_ID: AccountId = 2;
// Transactions per second sent to the gateway. This rate makes each block contain ~10 transactions
// with the set [TimeoutsConfig] .
pub const TPS: u64 = 2;
pub const N_TXS_IN_FIRST_BLOCK: usize = 2;

const PAID_FEE_ON_L1: U256 = U256::from_be_slice(b"paid"); // Arbitrary value.

pub type CreateRpcTxsFn = fn(&mut MultiAccountTransactionGenerator) -> Vec<RpcTransaction>;
pub type TestTxHashesFn = fn(&[TransactionHash]) -> Vec<TransactionHash>;
pub type ExpectedContentId = Felt;

pub trait TestScenario {
    fn create_txs(
        &self,
        tx_generator: &mut MultiAccountTransactionGenerator,
        account_id: AccountId,
    ) -> (Vec<RpcTransaction>, Vec<L1HandlerTransaction>);

    fn n_txs(&self) -> usize;
}

pub struct ConsensusTxs {
    pub n_invoke_txs: usize,
    pub n_l1_handler_txs: usize,
}

impl TestScenario for ConsensusTxs {
    fn create_txs(
        &self,
        tx_generator: &mut MultiAccountTransactionGenerator,
        account_id: AccountId,
    ) -> (Vec<RpcTransaction>, Vec<L1HandlerTransaction>) {
        (
            create_invoke_txs(tx_generator, account_id, self.n_invoke_txs),
            create_l1_handler_txs(tx_generator, self.n_l1_handler_txs),
        )
    }

    fn n_txs(&self) -> usize {
        self.n_invoke_txs + self.n_l1_handler_txs
    }
}

pub struct BootstrapTxs;

impl TestScenario for BootstrapTxs {
    fn create_txs(
        &self,
        tx_generator: &mut MultiAccountTransactionGenerator,
        account_id: AccountId,
    ) -> (Vec<RpcTransaction>, Vec<L1HandlerTransaction>) {
        let txs = create_deploy_account_tx_and_invoke_tx(tx_generator, account_id);
        assert_eq!(
            txs.len(),
            N_TXS_IN_FIRST_BLOCK,
            "First block should contain exactly {} transactions, but {} transactions were created",
            N_TXS_IN_FIRST_BLOCK,
            txs.len(),
        );
        (txs, vec![])
    }

    fn n_txs(&self) -> usize {
        N_TXS_IN_FIRST_BLOCK
    }
}

// TODO(Tsabary/Shahak/Yair/AlonH): this function needs a proper cleaning.
#[allow(clippy::too_many_arguments)]
pub fn create_node_config(
    available_ports: &mut AvailablePorts,
    node_execution_id: NodeExecutionId,
    chain_info: ChainInfo,
    batcher_storage_config: StorageConfig,
    state_sync_storage_config: StorageConfig,
    class_manager_storage_config: FsClassStorageConfig,
    mut state_sync_config: StateSyncConfig,
    mut consensus_manager_config: ConsensusManagerConfig,
    mempool_p2p_config: MempoolP2pConfig,
    monitoring_endpoint_config: MonitoringEndpointConfig,
    component_config: ComponentConfig,
    base_layer_config: EthereumBaseLayerConfig,
    block_max_capacity_sierra_gas: GasAmount,
) -> (SequencerNodeConfig, ConfigPointersMap) {
    let validator_id =
        set_validator_id(&mut consensus_manager_config, node_execution_id.get_node_index());
    let recorder_url = consensus_manager_config.cende_config.recorder_url.clone();
    let fee_token_addresses = chain_info.fee_token_addresses.clone();
    let batcher_config = create_batcher_config(
        batcher_storage_config,
        chain_info.clone(),
        block_max_capacity_sierra_gas,
    );
    let gateway_config = create_gateway_config(chain_info.clone());
    let l1_scraper_config =
        L1ScraperConfig { chain_id: chain_info.chain_id.clone(), ..Default::default() };
    let l1_provider_config = L1ProviderConfig {
        provider_startup_height_override: Some(BlockNumber(1)),
        ..Default::default()
    };
    let mempool_config = create_mempool_config();
    let http_server_config =
        create_http_server_config(available_ports.get_next_local_host_socket());
    let class_manager_config = create_class_manager_config(class_manager_storage_config);
    state_sync_config.storage_config = state_sync_storage_config;

    // Update config pointer values.
    let mut config_pointers_map = ConfigPointersMap::new(CONFIG_POINTERS.clone());
    config_pointers_map.change_target_value(
        "chain_id",
        to_value(chain_info.chain_id).expect("Failed to serialize ChainId"),
    );
    config_pointers_map.change_target_value(
        "eth_fee_token_address",
        to_value(fee_token_addresses.eth_fee_token_address)
            .expect("Failed to serialize ContractAddress"),
    );
    config_pointers_map.change_target_value(
        "strk_fee_token_address",
        to_value(fee_token_addresses.strk_fee_token_address)
            .expect("Failed to serialize ContractAddress"),
    );
    config_pointers_map.change_target_value(
        "validator_id",
        to_value(validator_id).expect("Failed to serialize ContractAddress"),
    );
    config_pointers_map.change_target_value(
        "recorder_url",
        to_value(recorder_url).expect("Failed to serialize Url"),
    );
    (
        SequencerNodeConfig {
            base_layer_config,
            batcher_config,
            class_manager_config,
            consensus_manager_config,
            gateway_config,
            http_server_config,
            mempool_config,
            mempool_p2p_config,
            monitoring_endpoint_config,
            state_sync_config,
            components: component_config,
            l1_scraper_config,
            l1_provider_config,
            ..Default::default()
        },
        config_pointers_map,
    )
}

pub(crate) fn create_consensus_manager_configs_from_network_configs(
    network_configs: Vec<NetworkConfig>,
    n_composed_nodes: usize,
    chain_id: &ChainId,
) -> Vec<ConsensusManagerConfig> {
    // TODO(Matan, Dan): set reasonable default timeouts.
    let mut timeouts = TimeoutsConfig::default();
    timeouts.precommit_timeout *= 3;
    timeouts.prevote_timeout *= 3;
    timeouts.proposal_timeout *= 3;

    let num_validators = u64::try_from(n_composed_nodes).unwrap();

    network_configs
        .into_iter()
        // TODO(Matan): Get config from default config file.
        .map(|network_config| ConsensusManagerConfig {
            network_config,
            immediate_active_height: BlockNumber(1),
            consensus_config: ConsensusConfig {
                // TODO(Matan, Dan): Set the right amount
                startup_delay: Duration::from_secs(15),
                timeouts: timeouts.clone(),
                ..Default::default()
            },
            context_config: ContextConfig {
                num_validators,
                chain_id: chain_id.clone(),
                builder_address: ContractAddress::from(4_u128),
                ..Default::default()
            },
            cende_config: CendeConfig{
                skip_write_height: Some(BlockNumber(1)),
                ..Default::default()
            },
            eth_to_strk_oracle_config: EthToStrkOracleConfig {
                base_url: Url::parse("https://eth_to_strk_oracle_url")
                    .expect("Should be a valid URL"),
                    ..Default::default()
            },
            ..Default::default()
        })
        .collect()
}

// Creates a local recorder server that always returns a success status.
pub fn spawn_success_recorder(socket_address: SocketAddr) -> JoinHandle<()> {
    tokio::spawn(async move {
        let router = Router::new().route(
            RECORDER_WRITE_BLOB_PATH,
            post(move || {
                async {
                    debug!("Received a request to write a blob.");
                    StatusCode::OK.to_string()
                }
                .instrument(tracing::debug_span!("success recorder write_blob"))
            }),
        );
        axum::Server::bind(&socket_address).serve(router.into_make_service()).await.unwrap();
    })
}

pub fn spawn_local_success_recorder(port: u16) -> (Url, JoinHandle<()>) {
    // [127, 0, 0, 1] is the localhost IP address.
    let socket_address = SocketAddr::from(([127, 0, 0, 1], port));
    // TODO(Tsabary): create a socket-to-url function.
    let url = Url::parse(&format!("http://{}", socket_address)).unwrap();
    let join_handle = spawn_success_recorder(socket_address);
    (url, join_handle)
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

/// Creates a multi-account transaction generator for the integration test.
pub fn create_integration_test_tx_generator() -> MultiAccountTransactionGenerator {
    let mut tx_generator: MultiAccountTransactionGenerator =
        MultiAccountTransactionGenerator::new();

    let account =
        FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1(RunnableCairo1::Casm));
    tx_generator.register_undeployed_account(account, ContractAddressSalt(Felt::ZERO));
    tx_generator
}

/// Creates a multi-account transaction generator for the flow test.
pub fn create_flow_test_tx_generator() -> MultiAccountTransactionGenerator {
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

pub fn create_multiple_account_txs(
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

/// Creates and sends more transactions than can fit in a block.
pub fn create_many_invoke_txs(
    tx_generator: &mut MultiAccountTransactionGenerator,
) -> Vec<RpcTransaction> {
    const N_TXS: usize = 15;
    create_invoke_txs(tx_generator, ACCOUNT_ID_1, N_TXS)
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
    recipient: &Contract,
) -> Vec<RpcTransaction> {
    let funding_tx = funding_account.generate_transfer(recipient);
    vec![funding_tx]
}

/// Generates a deploy account transaction followed by an invoke transaction from the same account.
/// The first invoke_tx can be inserted to the first block right after the deploy_tx due to
/// the skip_validate feature. This feature allows the gateway to accept this transaction although
/// the account does not exist yet.
pub fn create_deploy_account_tx_and_invoke_tx(
    tx_generator: &mut MultiAccountTransactionGenerator,
    account_id: AccountId,
) -> Vec<RpcTransaction> {
    let undeployed_account_tx_generator = tx_generator.account_with_id_mut(account_id);
    assert!(!undeployed_account_tx_generator.is_deployed());
    let deploy_tx = undeployed_account_tx_generator.generate_deploy_account();
    let invoke_tx = undeployed_account_tx_generator.generate_invoke_with_tip(1);
    vec![deploy_tx, invoke_tx]
}

pub fn create_invoke_txs(
    tx_generator: &mut MultiAccountTransactionGenerator,
    account_id: AccountId,
    n_txs: usize,
) -> Vec<RpcTransaction> {
    (0..n_txs)
        .map(|_| tx_generator.account_with_id_mut(account_id).generate_invoke_with_tip(1))
        .collect()
}

pub fn create_l1_handler_txs(
    tx_generator: &mut MultiAccountTransactionGenerator,
    n_txs: usize,
) -> Vec<L1HandlerTransaction> {
    (0..n_txs).map(|_| tx_generator.create_l1_handler_tx()).collect()
}

pub async fn send_message_to_l2_and_calculate_tx_hash(
    l1_handler: L1HandlerTransaction,
    starknet_l1_contract: &StarknetL1Contract,
    chain_id: &ChainId,
) -> TransactionHash {
    send_message_to_l2(&l1_handler, starknet_l1_contract).await;
    l1_handler.calculate_transaction_hash(chain_id, &l1_handler.version).unwrap()
}

/// Converts a given [L1 handler transaction](L1HandlerTransaction) to match the interface of the
/// given [starknet l1 contract](StarknetL1Contract), and triggers the L1 entry point which sends
/// the message to L2.
pub(crate) async fn send_message_to_l2(
    l1_handler: &L1HandlerTransaction,
    starknet_l1_contract: &StarknetL1Contract,
) {
    println!("yael L1 handler tx {:?}", l1_handler);
    let l2_contract_address = l1_handler.contract_address.0.key().to_hex_string().parse().unwrap();
    let l2_entry_point = l1_handler.entry_point_selector.0.to_hex_string().parse().unwrap();

    // The calldata of an L1 handler transaction consists of the L1 sender address followed by the
    // transaction payload. We remove the sender address to extract the message payload.
    let payload =
        l1_handler.calldata.0[1..].iter().map(|x| x.to_hex_string().parse().unwrap()).collect();
    let msg = starknet_l1_contract.sendMessageToL2(l2_contract_address, l2_entry_point, payload);

    let _tx_receipt = msg
        // Sets a non-zero fee to be paid on L1.
        .value(PAID_FEE_ON_L1)
        // Sends the transaction to the Starknet L1 contract. For debugging purposes, replace
        // `.send()` with `.call_raw()` to retrieve detailed error messages from L1.
        .send().await.expect("Transaction submission to Starknet L1 contract failed.")
        // Waits until the transaction is received on L1 and then fetches its receipt.
        .get_receipt().await.expect("Transaction was not received on L1 or receipt retrieval failed.");
}

async fn send_rpc_txs<'a, Fut>(
    rpc_txs: Vec<RpcTransaction>,
    send_rpc_tx_fn: &'a mut dyn Fn(RpcTransaction) -> Fut,
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
pub async fn run_test_scenario<'a, Fut>(
    tx_generator: &mut MultiAccountTransactionGenerator,
    create_rpc_txs_fn: CreateRpcTxsFn,
    l1_handler_txs: Vec<L1HandlerTransaction>,
    send_rpc_tx_fn: &'a mut dyn Fn(RpcTransaction) -> Fut,
    test_tx_hashes_fn: TestTxHashesFn,
    chain_id: &ChainId,
) -> Vec<TransactionHash>
where
    Fut: Future<Output = TransactionHash> + 'a,
{
    let mut tx_hashes: Vec<TransactionHash> = l1_handler_txs
        .iter()
        .map(|l1_handler| {
            l1_handler.calculate_transaction_hash(chain_id, &l1_handler.version).unwrap()
        })
        .collect();

    let rpc_txs = create_rpc_txs_fn(tx_generator);
    tx_hashes.extend(send_rpc_txs(rpc_txs, send_rpc_tx_fn).await);
    test_tx_hashes_fn(&tx_hashes)
}

pub fn test_multiple_account_txs(tx_hashes: &[TransactionHash]) -> Vec<TransactionHash> {
    // Return the transaction hashes in the order they should be given by the mempool:
    // Transactions from the same account are ordered by nonce; otherwise, higher tips are given
    // priority.
    assert!(
        tx_hashes.len() == 3,
        "Unexpected number of transactions sent in the test scenario. Found {} transactions",
        tx_hashes.len()
    );
    vec![tx_hashes[2], tx_hashes[0], tx_hashes[1]]
}

pub fn test_many_invoke_txs(tx_hashes: &[TransactionHash]) -> Vec<TransactionHash> {
    assert!(
        tx_hashes.len() == 15,
        "Unexpected number of transactions sent in the test scenario. Found {} transactions",
        tx_hashes.len()
    );
    // Only 12 transactions make it into the block (because the block is full).
    tx_hashes[..12].to_vec()
}

/// Returns a list of the transaction hashes, in the order they are expected to be in the mempool.
pub async fn send_consensus_txs<'a, 'b, FutA, FutB>(
    tx_generator: &mut MultiAccountTransactionGenerator,
    account_id: AccountId,
    test_scenario: &impl TestScenario,
    send_rpc_tx_fn: &'a mut dyn Fn(RpcTransaction) -> FutA,
    send_l1_handler_tx_fn: &'b mut dyn Fn(L1HandlerTransaction) -> FutB,
) -> Vec<TransactionHash>
where
    FutA: Future<Output = TransactionHash> + 'a,
    FutB: Future<Output = TransactionHash> + 'b,
{
    let n_txs = test_scenario.n_txs();
    info!("Sending {n_txs} txs.");

    let (rpc_txs, l1_txs) = test_scenario.create_txs(tx_generator, account_id);
    let mut tx_hashes = Vec::new();
    let l1_handler_tx_hashes = join_all(l1_txs.into_iter().map(send_l1_handler_tx_fn)).await;
    tracing::info!("Sent L1 handlers with tx hashes: {l1_handler_tx_hashes:?}");
    tx_hashes.extend(l1_handler_tx_hashes);
    tx_hashes.extend(send_rpc_txs(rpc_txs, send_rpc_tx_fn).await);
    assert_eq!(tx_hashes.len(), n_txs);
    tx_hashes
}

pub fn create_gateway_config(chain_info: ChainInfo) -> GatewayConfig {
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
    block_max_capacity_sierra_gas: GasAmount,
) -> BatcherConfig {
    // TODO(Arni): Create BlockBuilderConfig create for testing method and use here.
    let concurrency_enabled = true;
    BatcherConfig {
        storage: batcher_storage_config,
        block_builder_config: BlockBuilderConfig {
            chain_info,
            bouncer_config: BouncerConfig {
                block_max_capacity: BouncerWeights {
                    sierra_gas: block_max_capacity_sierra_gas,
                    ..Default::default()
                },
            },
            execute_config: TransactionExecutorConfig::create_for_testing(concurrency_enabled),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn create_mempool_config() -> MempoolConfig {
    MempoolConfig { transaction_ttl: Duration::from_secs(5 * 60), ..Default::default() }
}

pub fn create_class_manager_config(
    class_storage_config: FsClassStorageConfig,
) -> FsClassManagerConfig {
    let cached_class_storage_config =
        CachedClassStorageConfig { class_cache_size: 100, deprecated_class_cache_size: 100 };
    let class_manager_config = ClassManagerConfig { cached_class_storage_config };
    FsClassManagerConfig { class_manager_config, class_storage_config }
}

fn set_validator_id(
    consensus_manager_config: &mut ConsensusManagerConfig,
    node_index: usize,
) -> ValidatorId {
    let validator_id = ValidatorId::try_from(
        Felt::from(consensus_manager_config.consensus_config.validator_id) + Felt::from(node_index),
    )
    .unwrap();
    consensus_manager_config.consensus_config.validator_id = validator_id;
    validator_id
}

pub fn create_state_sync_configs(
    state_sync_storage_config: StorageConfig,
    ports: Vec<u16>,
) -> Vec<StateSyncConfig> {
    create_connected_network_configs(ports)
        .into_iter()
        .map(|network_config| StateSyncConfig {
            storage_config: state_sync_storage_config.clone(),
            network_config,
            ..Default::default()
        })
        .collect()
}

/// Stores tx hashes streamed so far.
/// Assumes that rounds are monotonically increasing and that the last round is the chosen one.
#[derive(Debug, Default)]
pub struct AccumulatedTransactions {
    pub latest_block_number: BlockNumber,
    pub round: u32,
    pub accumulated_tx_hashes: Vec<TransactionHash>,
    // Will be added when next height starts.
    current_round_tx_hashes: Vec<TransactionHash>,
}

impl AccumulatedTransactions {
    pub fn add_transactions(
        &mut self,
        height: BlockNumber,
        round: u32,
        tx_hashes: &[TransactionHash],
    ) {
        self.validate_coherent_height_and_round(height, round);
        if self.latest_block_number < height {
            info!(
                "New height started, total {} txs streamed from block {}.",
                self.current_round_tx_hashes.len(),
                self.latest_block_number
            );
            self.latest_block_number = height;
            self.round = round;
            self.accumulated_tx_hashes.append(&mut self.current_round_tx_hashes);
            self.current_round_tx_hashes = tx_hashes.to_vec();
        } else if self.latest_block_number == height && self.round < round {
            info!(
                "New round started ({}). Dropping {} txs of round {}. Adding {} pending txs to \
                 block {})",
                round,
                self.current_round_tx_hashes.len(),
                self.round,
                tx_hashes.len(),
                height,
            );
            self.round = round;
            self.current_round_tx_hashes = tx_hashes.to_vec();
        } else {
            info!(
                "Adding {} streamed txs in block {} round {}.",
                tx_hashes.len(),
                self.latest_block_number,
                self.round
            );
            self.current_round_tx_hashes.extend_from_slice(tx_hashes);
        }
    }

    fn validate_coherent_height_and_round(&self, height: BlockNumber, round: u32) {
        if self.latest_block_number > height {
            panic!("Expected height to be greater or equal to the last height with transactions.");
        }
        if self.latest_block_number == height && self.round > round {
            panic!("Expected round to be greater or equal to the last round.");
        }
    }
}
