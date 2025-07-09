use std::future::Future;
use std::net::SocketAddr;
use std::time::Duration;

use apollo_batcher::block_builder::BlockBuilderConfig;
use apollo_batcher::config::BatcherConfig;
use apollo_batcher::pre_confirmed_cende_client::RECORDER_WRITE_PRE_CONFIRMED_BLOCK_PATH;
use apollo_class_manager::class_storage::CachedClassStorageConfig;
use apollo_class_manager::config::{
    ClassManagerConfig,
    FsClassManagerConfig,
    FsClassStorageConfig,
};
use apollo_config::converters::UrlAndHeaders;
use apollo_consensus::config::{ConsensusConfig, TimeoutsConfig};
use apollo_consensus::types::ValidatorId;
use apollo_consensus_manager::config::ConsensusManagerConfig;
use apollo_consensus_orchestrator::cende::{CendeConfig, RECORDER_WRITE_BLOB_PATH};
use apollo_consensus_orchestrator::config::ContextConfig;
use apollo_gateway::config::{
    GatewayConfig,
    StatefulTransactionValidatorConfig,
    StatelessTransactionValidatorConfig,
};
use apollo_http_server::test_utils::create_http_server_config;
use apollo_infra_utils::test_utils::AvailablePorts;
use apollo_l1_endpoint_monitor::monitor::L1EndpointMonitorConfig;
use apollo_l1_gas_price::eth_to_strk_oracle::{EthToStrkOracleConfig, ETH_TO_STRK_QUANTIZATION};
use apollo_l1_gas_price::l1_gas_price_provider::L1GasPriceProviderConfig;
use apollo_l1_gas_price_types::DEFAULT_ETH_TO_FRI_RATE;
use apollo_l1_provider::l1_scraper::L1ScraperConfig;
use apollo_l1_provider::L1ProviderConfig;
use apollo_mempool::config::MempoolConfig;
use apollo_mempool_p2p::config::MempoolP2pConfig;
use apollo_monitoring_endpoint::config::MonitoringEndpointConfig;
use apollo_network::network_manager::test_utils::create_connected_network_configs;
use apollo_network::NetworkConfig;
use apollo_node::config::component_config::ComponentConfig;
use apollo_node::config::definitions::ConfigPointersMap;
use apollo_node::config::node_config::{SequencerNodeConfig, CONFIG_POINTERS};
use apollo_rpc::RpcConfig;
use apollo_state_sync::config::StateSyncConfig;
use apollo_storage::StorageConfig;
use axum::extract::Query;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use blockifier::blockifier::config::WorkerPoolConfig;
#[cfg(feature = "cairo_native")]
use blockifier::blockifier::config::{CairoNativeRunConfig, ContractClassManagerConfig};
use blockifier::bouncer::{BouncerConfig, BouncerWeights, BuiltinWeights};
use blockifier::context::ChainInfo;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::FeatureContract;
use mempool_test_utils::starknet_api_test_utils::{AccountId, MultiAccountTransactionGenerator};
use papyrus_base_layer::ethereum_base_layer_contract::{
    EthereumBaseLayerConfig,
    L1ToL2MessageArgs,
    StarknetL1Contract,
};
use serde::Deserialize;
use serde_json::{json, to_value};
use starknet_api::block::BlockNumber;
use starknet_api::contract_address;
use starknet_api::core::{ChainId, ContractAddress};
use starknet_api::execution_resources::GasAmount;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::fields::ContractAddressSalt;
use starknet_api::transaction::{TransactionHash, TransactionHasher};
use starknet_types_core::felt::Felt;
use tokio::task::JoinHandle;
use tracing::{debug, info, Instrument};
use url::Url;

use crate::state_reader::StorageTestConfig;

pub const ACCOUNT_ID_0: AccountId = 0;
pub const ACCOUNT_ID_1: AccountId = 1;
pub const NEW_ACCOUNT_SALT: ContractAddressSalt = ContractAddressSalt(Felt::THREE);
pub const UNDEPLOYED_ACCOUNT_ID: AccountId = 2;
// Transactions per second sent to the gateway. This rate makes each block contain ~15 transactions
// with the set [TimeoutsConfig] .
pub const TPS: u64 = 3;
pub const N_TXS_IN_FIRST_BLOCK: usize = 2;

pub type CreateRpcTxsFn = fn(&mut MultiAccountTransactionGenerator) -> Vec<RpcTransaction>;
pub type CreateL1ToL2MessagesArgsFn =
    fn(&mut MultiAccountTransactionGenerator) -> Vec<L1ToL2MessageArgs>;
pub type TestTxHashesFn = fn(&[TransactionHash]) -> Vec<TransactionHash>;

pub trait TestScenario {
    fn create_txs(
        &self,
        tx_generator: &mut MultiAccountTransactionGenerator,
        account_id: AccountId,
    ) -> (Vec<RpcTransaction>, Vec<L1ToL2MessageArgs>);

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
    ) -> (Vec<RpcTransaction>, Vec<L1ToL2MessageArgs>) {
        (
            create_invoke_txs(tx_generator, account_id, self.n_invoke_txs),
            create_l1_to_l2_messages_args(tx_generator, self.n_l1_handler_txs),
        )
    }

    fn n_txs(&self) -> usize {
        self.n_invoke_txs + self.n_l1_handler_txs
    }
}

pub struct DeclareTx;

impl TestScenario for DeclareTx {
    fn create_txs(
        &self,
        tx_generator: &mut MultiAccountTransactionGenerator,
        account_id: AccountId,
    ) -> (Vec<RpcTransaction>, Vec<L1ToL2MessageArgs>) {
        let declare_tx = tx_generator.account_with_id_mut(account_id).generate_declare();
        (vec![declare_tx], vec![])
    }

    fn n_txs(&self) -> usize {
        1
    }
}

pub struct DeployAndInvokeTxs;

impl TestScenario for DeployAndInvokeTxs {
    fn create_txs(
        &self,
        tx_generator: &mut MultiAccountTransactionGenerator,
        account_id: AccountId,
    ) -> (Vec<RpcTransaction>, Vec<L1ToL2MessageArgs>) {
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
    chain_info: ChainInfo,
    storage_config: StorageTestConfig,
    mut state_sync_config: StateSyncConfig,
    consensus_manager_config: ConsensusManagerConfig,
    mempool_p2p_config: MempoolP2pConfig,
    monitoring_endpoint_config: MonitoringEndpointConfig,
    component_config: ComponentConfig,
    base_layer_config: EthereumBaseLayerConfig,
    block_max_capacity_sierra_gas: GasAmount,
    validator_id: ValidatorId,
    allow_bootstrap_txs: bool,
) -> (SequencerNodeConfig, ConfigPointersMap) {
    let recorder_url = consensus_manager_config.cende_config.recorder_url.clone();
    let fee_token_addresses = chain_info.fee_token_addresses.clone();
    let batcher_config = create_batcher_config(
        storage_config.batcher_storage_config,
        chain_info.clone(),
        block_max_capacity_sierra_gas,
    );
    let validate_non_zero_resource_bounds = !allow_bootstrap_txs;
    let gateway_config =
        create_gateway_config(chain_info.clone(), validate_non_zero_resource_bounds);
    let l1_scraper_config =
        L1ScraperConfig { chain_id: chain_info.chain_id.clone(), ..Default::default() };
    let l1_provider_config = L1ProviderConfig {
        provider_startup_height_override: Some(BlockNumber(1)),
        ..Default::default()
    };
    let l1_endpoint_monitor_config = L1EndpointMonitorConfig {
        // This is the Anvil URL, initialized at the callsite.
        // TODO(Gilad): make this explicit in the Anvil refactor.
        ordered_l1_endpoint_urls: vec![base_layer_config.node_url.clone()],
    };
    let override_gas_price_threshold_check = allow_bootstrap_txs;
    let mempool_config = create_mempool_config(override_gas_price_threshold_check);
    let l1_gas_price_provider_config = L1GasPriceProviderConfig {
        // Use newly minted blocks on Anvil to be used for gas price calculations.
        lag_margin_seconds: 0,
        ..Default::default()
    };
    let http_server_config =
        create_http_server_config(available_ports.get_next_local_host_socket());
    let class_manager_config =
        create_class_manager_config(storage_config.class_manager_storage_config);
    state_sync_config.storage_config = storage_config.state_sync_storage_config;
    state_sync_config.rpc_config.chain_id = chain_info.chain_id.clone();
    let starknet_url = state_sync_config.rpc_config.starknet_url.clone();

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
    config_pointers_map.change_target_value(
        "starknet_url",
        to_value(starknet_url).expect("Failed to serialize starknet_url"),
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
            l1_endpoint_monitor_config,
            l1_gas_price_provider_config,
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
                url_header_list: Some(vec![
                    UrlAndHeaders{
                        url: Url::parse("https://eth_to_strk_oracle_url").expect("Should be a valid URL"), 
                        headers: Default::default(),
                    }
                ]),
                ..Default::default()
            },
            assume_no_malicious_validators: true,
            ..Default::default()
        })
        .collect()
}

// Creates a local recorder server that always returns a success status.
pub fn spawn_success_recorder(socket_address: SocketAddr) -> JoinHandle<()> {
    tokio::spawn(async move {
        let router = Router::new()
            .route(
                RECORDER_WRITE_BLOB_PATH,
                post(move || {
                    async {
                        debug!("Received a request to write a blob.");
                        StatusCode::OK.to_string()
                    }
                    .instrument(tracing::debug_span!("success recorder write_blob"))
                }),
            )
            .route(
                RECORDER_WRITE_PRE_CONFIRMED_BLOCK_PATH,
                post(move || {
                    async {
                        debug!("Received a request to write a pre-confirmed block.");
                        StatusCode::OK.to_string()
                    }
                    .instrument(tracing::debug_span!("success recorder write_pre_confirmed_block"))
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

/// Fake eth to strk oracle endpoint.
const ETH_TO_STRK_ORACLE_PATH: &str = "/eth_to_strk_oracle";

/// Expected query parameters.
#[derive(Deserialize)]
struct EthToStrkOracleQuery {
    timestamp: u64,
}

/// Returns a fake eth to fri rate response.
async fn get_price(Query(query): Query<EthToStrkOracleQuery>) -> Json<serde_json::Value> {
    // This value must be large enough so that conversion for ETH to STRK is not zero (e.g. for gas
    // prices). We set a value a bit higher than the min needed to avoid test failures due to
    // small changes.
    //
    // TODO(Asmaa): Retrun timestamp as price once we start mocking out time in the
    // tests.
    let price = format!("0x{:x}", DEFAULT_ETH_TO_FRI_RATE);
    let response = json!({ "timestamp": query.timestamp ,"price": price, "decimals": ETH_TO_STRK_QUANTIZATION });
    Json(response)
}

/// Spawns a local fake eth to fri oracle server.
pub fn spawn_eth_to_strk_oracle_server(socket_address: SocketAddr) -> JoinHandle<()> {
    tokio::spawn(async move {
        let router = Router::new().route(ETH_TO_STRK_ORACLE_PATH, get(get_price));
        axum::Server::bind(&socket_address).serve(router.into_make_service()).await.unwrap();
    })
}

/// Starts the fake eth to fri oracle server and returns its URL and handle.
pub fn spawn_local_eth_to_strk_oracle(port: u16) -> (UrlAndHeaders, JoinHandle<()>) {
    let socket_address = SocketAddr::from(([127, 0, 0, 1], port));
    let url = Url::parse(&format!("http://{}{}", socket_address, ETH_TO_STRK_ORACLE_PATH)).unwrap();
    let url_and_headers = UrlAndHeaders {
        url,
        headers: Default::default(), // No additional headers needed for this test.
    };
    let join_handle = spawn_eth_to_strk_oracle_server(socket_address);
    (url_and_headers, join_handle)
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

pub fn create_l1_to_l2_messages_args(
    tx_generator: &mut MultiAccountTransactionGenerator,
    n_txs: usize,
) -> Vec<L1ToL2MessageArgs> {
    (0..n_txs).map(|_| tx_generator.create_l1_to_l2_message_args()).collect()
}

pub async fn send_message_to_l2_and_calculate_tx_hash(
    send_message_to_l2_args: L1ToL2MessageArgs,
    starknet_l1_contract: &StarknetL1Contract,
    chain_id: &ChainId,
) -> TransactionHash {
    starknet_l1_contract.send_message_to_l2(&send_message_to_l2_args).await;
    send_message_to_l2_args
        .tx
        .calculate_transaction_hash(chain_id, &send_message_to_l2_args.tx.version)
        .unwrap()
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
    l1_to_l2_message_args: Vec<L1ToL2MessageArgs>,
    send_rpc_tx_fn: &'a mut dyn Fn(RpcTransaction) -> Fut,
    test_tx_hashes_fn: TestTxHashesFn,
    chain_id: &ChainId,
) -> Vec<TransactionHash>
where
    Fut: Future<Output = TransactionHash> + 'a,
{
    let mut tx_hashes: Vec<TransactionHash> = l1_to_l2_message_args
        .iter()
        .map(|args| args.tx.calculate_transaction_hash(chain_id, &args.tx.version).unwrap())
        .collect();

    let rpc_txs = create_rpc_txs_fn(tx_generator);
    tx_hashes.extend(send_rpc_txs(rpc_txs, send_rpc_tx_fn).await);
    test_tx_hashes_fn(&tx_hashes)
}

/// Returns a list of the transaction hashes, in the order they are expected to be in the mempool.
pub async fn send_consensus_txs<'a, 'b, FutA, FutB>(
    tx_generator: &mut MultiAccountTransactionGenerator,
    account_id: AccountId,
    test_scenario: &impl TestScenario,
    send_rpc_tx_fn: &'a mut dyn Fn(RpcTransaction) -> FutA,
    send_l1_handler_tx_fn: &'b mut dyn Fn(L1ToL2MessageArgs) -> FutB,
) -> Vec<TransactionHash>
where
    FutA: Future<Output = TransactionHash> + 'a,
    FutB: Future<Output = TransactionHash> + 'b,
{
    let n_txs = test_scenario.n_txs();
    info!("Sending {n_txs} txs.");

    let (rpc_txs, l1_txs) = test_scenario.create_txs(tx_generator, account_id);
    let mut tx_hashes = Vec::new();
    let mut l1_handler_tx_hashes = Vec::new();
    for l1_tx in l1_txs {
        l1_handler_tx_hashes.push(send_l1_handler_tx_fn(l1_tx).await);
    }
    // let l1_handler_tx_hashes = join_all(l1_txs.into_iter().map(send_l1_handler_tx_fn)).await;
    tracing::info!("Sent L1 handlers with tx hashes: {l1_handler_tx_hashes:?}");
    tx_hashes.extend(l1_handler_tx_hashes);
    tx_hashes.extend(send_rpc_txs(rpc_txs, send_rpc_tx_fn).await);
    assert_eq!(tx_hashes.len(), n_txs);
    tx_hashes
}

pub fn create_gateway_config(
    chain_info: ChainInfo,
    validate_non_zero_resource_bounds: bool,
) -> GatewayConfig {
    let stateless_tx_validator_config = StatelessTransactionValidatorConfig {
        validate_non_zero_resource_bounds,
        max_calldata_length: 10,
        max_signature_length: 2,
        ..Default::default()
    };
    let stateful_tx_validator_config =
        StatefulTransactionValidatorConfig { max_allowed_nonce_gap: 1000, ..Default::default() };

    GatewayConfig {
        stateless_tx_validator_config,
        stateful_tx_validator_config,
        chain_info,
        block_declare: false,
        authorized_declarer_accounts: None,
    }
}

pub fn create_batcher_config(
    batcher_storage_config: StorageConfig,
    chain_info: ChainInfo,
    block_max_capacity_sierra_gas: GasAmount,
) -> BatcherConfig {
    // TODO(Arni): Create BlockBuilderConfig create for testing method and use here.
    BatcherConfig {
        storage: batcher_storage_config,
        block_builder_config: BlockBuilderConfig {
            chain_info,
            bouncer_config: BouncerConfig {
                block_max_capacity: BouncerWeights {
                    sierra_gas: block_max_capacity_sierra_gas,
                    ..Default::default()
                },
                builtin_weights: BuiltinWeights::default(),
            },
            execute_config: WorkerPoolConfig::create_for_testing(),
            n_concurrent_txs: 3,
            ..Default::default()
        },
        #[cfg(feature = "cairo_native")]
        contract_class_manager_config: cairo_native_class_manager_config(),
        ..Default::default()
    }
}

pub fn create_mempool_config(override_gas_price_threshold_check: bool) -> MempoolConfig {
    MempoolConfig {
        transaction_ttl: Duration::from_secs(5 * 60),
        override_gas_price_threshold_check,
        ..Default::default()
    }
}

pub fn create_class_manager_config(
    class_storage_config: FsClassStorageConfig,
) -> FsClassManagerConfig {
    let cached_class_storage_config =
        CachedClassStorageConfig { class_cache_size: 100, deprecated_class_cache_size: 100 };
    let class_manager_config =
        ClassManagerConfig { cached_class_storage_config, ..Default::default() };
    FsClassManagerConfig { class_manager_config, class_storage_config }
}

pub fn set_validator_id(
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
    mut rpc_ports: Vec<u16>,
) -> Vec<StateSyncConfig> {
    create_connected_network_configs(ports)
        .into_iter()
        .map(|network_config| StateSyncConfig {
            storage_config: state_sync_storage_config.clone(),
            network_config: Some(network_config),
            rpc_config: RpcConfig {
                ip: [127, 0, 0, 1].into(),
                port: rpc_ports.remove(0),
                ..Default::default()
            },
            ..Default::default()
        })
        .collect()
}

#[cfg(feature = "cairo_native")]
fn cairo_native_class_manager_config() -> ContractClassManagerConfig {
    ContractClassManagerConfig {
        cairo_native_run_config: CairoNativeRunConfig {
            run_cairo_native: true,
            wait_on_native_compilation: true,
            panic_on_compilation_failure: true,
            ..Default::default()
        },
        ..Default::default()
    }
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
    pub fn start_round(&mut self, height: BlockNumber, round: u32) {
        self.validate_coherent_height_and_round(height, round);
        if self.latest_block_number < height {
            info!(
                "Starting height {}, total {} txs streamed from block {}.",
                height,
                self.current_round_tx_hashes.len(),
                self.latest_block_number
            );
            self.latest_block_number = height;
            self.round = round;
            self.accumulated_tx_hashes.append(&mut self.current_round_tx_hashes);
        } else if self.latest_block_number == height && self.round < round {
            info!(
                "New round started ({}). Dropping {} txs of round {} (height {}).",
                round,
                self.current_round_tx_hashes.len(),
                self.round,
                height,
            );
            self.round = round;
            self.current_round_tx_hashes.clear();
        }
    }

    pub fn add_transactions(&mut self, tx_hashes: &[TransactionHash]) {
        info!(
            "Adding {} txs to the current round: {}, height: {}.",
            tx_hashes.len(),
            self.round,
            self.latest_block_number
        );
        self.current_round_tx_hashes.extend_from_slice(tx_hashes);
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
