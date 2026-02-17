use std::future::Future;
use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use apollo_base_layer_tests::anvil_base_layer::AnvilBaseLayer;
use apollo_batcher::metrics::REVERTED_TRANSACTIONS;
use apollo_batcher::pre_confirmed_cende_client::RECORDER_WRITE_PRE_CONFIRMED_BLOCK_PATH;
use apollo_batcher_config::config::{BatcherConfig, BatcherStaticConfig, BlockBuilderConfig};
use apollo_class_manager_config::config::{
    CachedClassStorageConfig,
    ClassManagerConfig,
    ClassManagerDynamicConfig,
    ClassManagerStaticConfig,
    FsClassManagerConfig,
    FsClassStorageConfig,
};
use apollo_committer_config::config::ApolloCommitterConfig;
use apollo_config::converters::UrlAndHeaders;
use apollo_config_manager_config::config::ConfigManagerConfig;
use apollo_consensus_config::config::{
    ConsensusConfig,
    ConsensusDynamicConfig,
    ConsensusStaticConfig,
    TimeoutsConfig,
};
use apollo_consensus_config::ValidatorId;
use apollo_consensus_manager_config::config::ConsensusManagerConfig;
use apollo_consensus_orchestrator::cende::RECORDER_WRITE_BLOB_PATH;
use apollo_consensus_orchestrator_config::config::{
    CendeConfig,
    ContextConfig,
    ContextStaticConfig,
};
use apollo_gateway_config::config::{
    GatewayConfig,
    GatewayStaticConfig,
    StatefulTransactionValidatorConfig,
    StatelessTransactionValidatorConfig,
};
use apollo_http_server::test_utils::create_http_server_config;
use apollo_infra::trace_util::configure_tracing;
use apollo_infra_utils::test_utils::{AvailablePorts, TestIdentifier};
use apollo_l1_gas_price::eth_to_strk_oracle::ETH_TO_STRK_QUANTIZATION;
use apollo_l1_gas_price_provider_config::config::{
    EthToStrkOracleConfig,
    L1GasPriceProviderConfig,
    L1GasPriceScraperConfig,
};
use apollo_l1_gas_price_types::DEFAULT_ETH_TO_FRI_RATE;
use apollo_l1_provider_config::config::L1ProviderConfig;
use apollo_l1_scraper_config::config::L1ScraperConfig;
use apollo_mempool_config::config::{MempoolConfig, MempoolDynamicConfig, MempoolStaticConfig};
use apollo_mempool_p2p_config::config::MempoolP2pConfig;
use apollo_monitoring_endpoint_config::config::MonitoringEndpointConfig;
use apollo_network::network_manager::test_utils::create_connected_network_configs;
use apollo_network::NetworkConfig;
use apollo_node_config::component_config::ComponentConfig;
use apollo_node_config::component_execution_config::ExpectedComponentConfig;
use apollo_node_config::definitions::ConfigPointersMap;
use apollo_node_config::monitoring::MonitoringConfig;
use apollo_node_config::node_config::{SequencerNodeConfig, CONFIG_POINTERS};
use apollo_protobuf::consensus::DEFAULT_VALIDATOR_ID;
use apollo_rpc::RpcConfig;
use apollo_sierra_compilation_config::config::SierraCompilationConfig;
use apollo_staking_config::config::{
    CommitteeConfig,
    ConfiguredStaker,
    StakingManagerConfig,
    StakingManagerDynamicConfig,
};
use apollo_state_sync_config::config::{
    StateSyncConfig,
    StateSyncDynamicConfig,
    StateSyncStaticConfig,
};
use apollo_storage::db::DbConfig;
use apollo_storage::StorageConfig;
use axum::extract::Query;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{serve, Json, Router};
#[cfg(feature = "cairo_native")]
use blockifier::blockifier::config::CairoNativeRunConfig;
use blockifier::blockifier::config::{ContractClassManagerConfig, WorkerPoolConfig};
use blockifier::bouncer::{BouncerConfig, BouncerWeights};
use blockifier::context::ChainInfo;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::FeatureContract;
use mempool_test_utils::starknet_api_test_utils::{AccountId, MultiAccountTransactionGenerator};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use papyrus_base_layer::ethereum_base_layer_contract::EthereumBaseLayerConfig;
use pretty_assertions::assert_eq;
use serde::Deserialize;
use serde_json::{json, to_value};
use starknet_api::block::BlockNumber;
use starknet_api::core::{ChainId, ContractAddress};
use starknet_api::execution_resources::GasAmount;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::staking::StakingWeight;
use starknet_api::transaction::fields::ContractAddressSalt;
use starknet_api::transaction::{L1HandlerTransaction, TransactionHash, TransactionHasher};
use starknet_types_core::felt::Felt;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tracing::{debug, info, Instrument};
use url::Url;

use crate::flow_test_setup::{FlowSequencerSetup, FlowTestSetup, NUM_OF_SEQUENCERS};
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
    fn(&mut MultiAccountTransactionGenerator) -> Vec<L1HandlerTransaction>;
pub type TestTxHashesFn = fn(&[TransactionHash]) -> Vec<TransactionHash>;

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
        const SHOULD_REVERT: bool = false;
        (
            create_invoke_txs(tx_generator, account_id, self.n_invoke_txs),
            create_l1_to_l2_messages_args(tx_generator, self.n_l1_handler_txs, SHOULD_REVERT),
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
    ) -> (Vec<RpcTransaction>, Vec<L1HandlerTransaction>) {
        let declare_tx =
            tx_generator.account_with_id_mut(account_id).generate_declare_of_contract_class();
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

// TODO(Tsabary): clean the passed args.
#[allow(clippy::too_many_arguments)]
pub fn create_node_config(
    available_ports: &mut AvailablePorts,
    chain_info: ChainInfo,
    storage_config: StorageTestConfig,
    mut state_sync_config: StateSyncConfig,
    mut consensus_manager_config: ConsensusManagerConfig,
    eth_to_strk_oracle_config: EthToStrkOracleConfig,
    mempool_p2p_config: MempoolP2pConfig,
    monitoring_endpoint_config: MonitoringEndpointConfig,
    components: ComponentConfig,
    base_layer_config: EthereumBaseLayerConfig,
    block_max_capacity_gas: GasAmount,
    validator_id: ValidatorId,
    allow_bootstrap_txs: bool,
) -> (SequencerNodeConfig, ConfigPointersMap) {
    let recorder_url = consensus_manager_config.cende_config.recorder_url.clone();
    let fee_token_addresses = chain_info.fee_token_addresses.clone();
    let batcher_config = create_batcher_config(
        storage_config.batcher_storage_config,
        chain_info.clone(),
        block_max_capacity_gas,
    );
    let committer_config = ApolloCommitterConfig {
        db_path: storage_config.committer_db_path.clone(),
        ..Default::default()
    };
    let validate_non_zero_resource_bounds = !allow_bootstrap_txs;
    let gateway_config =
        create_gateway_config(chain_info.clone(), validate_non_zero_resource_bounds);
    let l1_scraper_config = L1ScraperConfig {
        chain_id: chain_info.chain_id.clone(),
        startup_rewind_time_seconds: Duration::from_secs(0),
        polling_interval_seconds: Duration::from_secs(1),
        ..Default::default()
    };
    let l1_provider_config = L1ProviderConfig {
        startup_sync_sleep_retry_interval_seconds: Duration::from_secs(0),
        l1_handler_cancellation_timelock_seconds: Duration::from_secs(0),
        l1_handler_consumption_timelock_seconds: Duration::from_secs(0),
        l1_handler_proposal_cooldown_seconds: Duration::from_secs(0),
        ..Default::default()
    };
    let validate_resource_bounds = !allow_bootstrap_txs;
    let mempool_config = create_mempool_config(validate_resource_bounds);
    let l1_gas_price_provider_config = L1GasPriceProviderConfig {
        // Use newly minted blocks on Anvil to be used for gas price calculations.
        lag_margin_seconds: Duration::from_secs(0),
        eth_to_strk_oracle_config,
        ..Default::default()
    };
    let http_server_config =
        create_http_server_config(available_ports.get_next_local_host_socket());
    let class_manager_config =
        create_class_manager_config(storage_config.class_manager_storage_config);
    state_sync_config.static_config.storage_config = storage_config.state_sync_storage_config;
    state_sync_config.static_config.rpc_config.chain_id = chain_info.chain_id.clone();
    let starknet_url = state_sync_config.static_config.rpc_config.starknet_url.clone();

    consensus_manager_config.consensus_manager_config.static_config.storage_config =
        storage_config.consensus_storage_config.clone();

    let l1_gas_price_scraper_config = L1GasPriceScraperConfig::default();
    let sierra_compiler_config = SierraCompilationConfig::default();

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
        to_value(&recorder_url).expect("Failed to serialize Url"),
    );
    config_pointers_map.change_target_value(
        "starknet_url",
        to_value(starknet_url).expect("Failed to serialize starknet_url"),
    );

    // A helper macro that wraps the config in `Some(...)` if `components.<field>` expects it;
    // otherwise returns `None`. Assumes `components` is in scope.
    macro_rules! wrap_if_component_config_expected {
        ($component_field:ident, $config_field:expr) => {{
            if components.$component_field.is_running_locally() {
                Some($config_field)
            } else {
                None
            }
        }};
    }

    // Retain only the required configs.
    let base_layer_config = Some(base_layer_config);
    let batcher_config = wrap_if_component_config_expected!(batcher, batcher_config);
    let class_manager_config =
        wrap_if_component_config_expected!(class_manager, class_manager_config);
    let config_manager_config = ConfigManagerConfig::disabled();
    let config_manager_config =
        wrap_if_component_config_expected!(config_manager, config_manager_config);
    let committer_config = wrap_if_component_config_expected!(committer, committer_config);
    let consensus_manager_config =
        wrap_if_component_config_expected!(consensus_manager, consensus_manager_config);
    let gateway_config = wrap_if_component_config_expected!(gateway, gateway_config);
    let http_server_config = wrap_if_component_config_expected!(http_server, http_server_config);
    let l1_gas_price_provider_config =
        wrap_if_component_config_expected!(l1_gas_price_provider, l1_gas_price_provider_config);
    let l1_gas_price_scraper_config =
        wrap_if_component_config_expected!(l1_gas_price_scraper, l1_gas_price_scraper_config);
    let l1_provider_config = wrap_if_component_config_expected!(l1_provider, l1_provider_config);
    let l1_scraper_config = wrap_if_component_config_expected!(l1_scraper, l1_scraper_config);
    let mempool_config = wrap_if_component_config_expected!(mempool, mempool_config);
    let mempool_p2p_config = wrap_if_component_config_expected!(mempool_p2p, mempool_p2p_config);
    let monitoring_endpoint_config =
        wrap_if_component_config_expected!(monitoring_endpoint, monitoring_endpoint_config);
    let monitoring_config = MonitoringConfig::default();
    let sierra_compiler_config =
        wrap_if_component_config_expected!(sierra_compiler, sierra_compiler_config);
    let state_sync_config = wrap_if_component_config_expected!(state_sync, state_sync_config);

    let sequencer_node_config = SequencerNodeConfig {
        base_layer_config,
        batcher_config,
        class_manager_config,
        committer_config,
        components,
        config_manager_config,
        consensus_manager_config,
        gateway_config,
        http_server_config,
        l1_gas_price_provider_config,
        l1_gas_price_scraper_config,
        l1_provider_config,
        l1_scraper_config,
        mempool_config,
        mempool_p2p_config,
        monitoring_endpoint_config,
        monitoring_config,
        sierra_compiler_config,
        state_sync_config,
    };

    sequencer_node_config.validate_node_config().expect("Generated node config should be valid.");

    (sequencer_node_config, config_pointers_map)
}

pub(crate) fn create_consensus_manager_configs_from_network_configs(
    network_configs: Vec<NetworkConfig>,
    n_composed_nodes: usize,
    chain_id: &ChainId,
) -> Vec<ConsensusManagerConfig> {
    let mut timeouts = TimeoutsConfig::default();
    // Scale by 2.0 for integration runs to avoid timeouts.
    timeouts.scale_by(2.0);

    // Create stakers config for epoch 0 with all validators.
    let stakers = (0..n_composed_nodes)
        .map(|i| {
            let address = ContractAddress::from(DEFAULT_VALIDATOR_ID + u64::try_from(i).unwrap());
            ConfiguredStaker {
                address,
                weight: StakingWeight(1),
                public_key: Felt::from(i),
                can_propose: true,
            }
        })
        .collect();
    let staking_manager_config = StakingManagerConfig {
        dynamic_config: StakingManagerDynamicConfig {
            default_committee: CommitteeConfig {
                start_epoch: 0,
                committee_size: n_composed_nodes,
                stakers,
            },
            override_committee: None,
        },
        static_config: Default::default(),
    };

    network_configs
        .into_iter()
        // TODO(Matan): Get config from default config file.
        .map(|network_config| ConsensusManagerConfig {
            network_config,
            consensus_manager_config: ConsensusConfig {
                dynamic_config: ConsensusDynamicConfig {
                    timeouts: timeouts.clone(),
                    ..Default::default()
                },
                static_config: ConsensusStaticConfig {
                    storage_config: StorageConfig { db_config: DbConfig{
                        path_prefix: "/data/consensus".into(),
                        enforce_file_exists: false,
                        ..Default::default()
                    },
                    ..Default::default() },
                    // TODO(Matan, Dan): Set the right amount
                    startup_delay: Duration::from_secs(15),
                },
            },
            context_config: ContextConfig {
                static_config: ContextStaticConfig {
                    chain_id: chain_id.clone(),
                    builder_address: ContractAddress::from(4_u128),
                    ..Default::default()
                },
                ..Default::default()
            },
            cende_config: CendeConfig {
                ..Default::default()
            },
            assume_no_malicious_validators: true,
            staking_manager_config: staking_manager_config.clone(),
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
        let listener = TcpListener::bind(socket_address).await.unwrap();
        serve(listener, router).await.unwrap();
    })
}

pub fn spawn_local_success_recorder(port: u16) -> (Url, JoinHandle<()>) {
    let socket_address = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), port);
    let url = Url::parse(&format!("http://{socket_address}")).unwrap();
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
    let price = format!("0x{DEFAULT_ETH_TO_FRI_RATE:x}");
    let response = json!({ "timestamp": query.timestamp ,"price": price, "decimals": ETH_TO_STRK_QUANTIZATION });
    Json(response)
}

/// Spawns a local fake eth to fri oracle server.
pub fn spawn_eth_to_strk_oracle_server(socket_address: SocketAddr) -> JoinHandle<()> {
    tokio::spawn(async move {
        let router = Router::new().route(ETH_TO_STRK_ORACLE_PATH, get(get_price));
        let listener = TcpListener::bind(socket_address).await.unwrap();
        serve(listener, router).await.unwrap();
    })
}

/// Starts the fake eth to fri oracle server and returns its URL and handle.
pub fn spawn_local_eth_to_strk_oracle(port: u16) -> (UrlAndHeaders, JoinHandle<()>) {
    let socket_address = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), port);
    let url = Url::parse(&format!("http://{socket_address}{ETH_TO_STRK_ORACLE_PATH}")).unwrap();
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
fn create_flow_test_tx_generator() -> MultiAccountTransactionGenerator {
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
    let invoke_tx = undeployed_account_tx_generator.generate_trivial_rpc_invoke_tx(1);
    vec![deploy_tx, invoke_tx]
}

pub fn create_invoke_txs(
    tx_generator: &mut MultiAccountTransactionGenerator,
    account_id: AccountId,
    n_txs: usize,
) -> Vec<RpcTransaction> {
    (0..n_txs)
        .map(|_| tx_generator.account_with_id_mut(account_id).generate_trivial_rpc_invoke_tx(1))
        .collect()
}

pub fn create_l1_to_l2_messages_args(
    tx_generator: &mut MultiAccountTransactionGenerator,
    n_txs: usize,
    should_revert: bool,
) -> Vec<L1HandlerTransaction> {
    (0..n_txs).map(|_| tx_generator.create_l1_to_l2_message_args(should_revert)).collect()
}

pub async fn send_message_to_l2_and_calculate_tx_hash(
    l1_handler: L1HandlerTransaction,
    anvil_base_layer: &AnvilBaseLayer,
    chain_id: &ChainId,
) -> TransactionHash {
    anvil_base_layer.send_message_to_l2(&l1_handler).await;
    l1_handler.calculate_transaction_hash(chain_id, &l1_handler.version).unwrap()
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
async fn run_test_scenario<'a, Fut>(
    tx_generator: &mut MultiAccountTransactionGenerator,
    create_rpc_txs_fn: CreateRpcTxsFn,
    l1_handlers: Vec<L1HandlerTransaction>,
    send_rpc_tx_fn: &'a mut dyn Fn(RpcTransaction) -> Fut,
    test_tx_hashes_fn: TestTxHashesFn,
    chain_id: &ChainId,
) -> Vec<TransactionHash>
where
    Fut: Future<Output = TransactionHash> + 'a,
{
    let mut tx_hashes: Vec<TransactionHash> = l1_handlers
        .iter()
        .map(|l1_handler| {
            l1_handler.calculate_transaction_hash(chain_id, &l1_handler.version).unwrap()
        })
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
        validate_resource_bounds: validate_non_zero_resource_bounds,
        max_calldata_length: 19,
        max_signature_length: 2,
        ..Default::default()
    };
    let stateful_tx_validator_config = StatefulTransactionValidatorConfig {
        max_allowed_nonce_gap: 1000,
        validate_resource_bounds: validate_non_zero_resource_bounds,
        ..Default::default()
    };
    let contract_class_manager_config = ContractClassManagerConfig::default();

    GatewayConfig {
        static_config: GatewayStaticConfig {
            stateless_tx_validator_config,
            stateful_tx_validator_config,
            contract_class_manager_config,
            chain_info,
            block_declare: false,
            authorized_declarer_accounts: None,
        },
    }
}

pub fn create_batcher_config(
    batcher_storage_config: StorageConfig,
    chain_info: ChainInfo,
    block_max_capacity_gas: GasAmount,
) -> BatcherConfig {
    // TODO(Arni): Create BlockBuilderConfig create for testing method and use here.
    BatcherConfig {
        static_config: BatcherStaticConfig {
            storage: batcher_storage_config,
            block_builder_config: BlockBuilderConfig {
                chain_info,
                bouncer_config: BouncerConfig {
                    block_max_capacity: BouncerWeights {
                        sierra_gas: block_max_capacity_gas,
                        proving_gas: block_max_capacity_gas,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                execute_config: WorkerPoolConfig::create_for_testing(),
                n_concurrent_txs: 3,
                ..Default::default()
            },
            #[cfg(feature = "cairo_native")]
            contract_class_manager_config: cairo_native_class_manager_config(),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn create_mempool_config(validate_resource_bounds: bool) -> MempoolConfig {
    MempoolConfig {
        dynamic_config: MempoolDynamicConfig { transaction_ttl: Duration::from_secs(5 * 60) },
        static_config: MempoolStaticConfig { validate_resource_bounds, ..Default::default() },
    }
}

pub fn create_class_manager_config(
    class_storage_config: FsClassStorageConfig,
) -> FsClassManagerConfig {
    let cached_class_storage_config =
        CachedClassStorageConfig { class_cache_size: 100, deprecated_class_cache_size: 100 };
    let class_manager_config =
        ClassManagerConfig { cached_class_storage_config, ..Default::default() };
    let static_config = ClassManagerStaticConfig { class_manager_config, class_storage_config };
    FsClassManagerConfig { static_config, dynamic_config: ClassManagerDynamicConfig::default() }
}

pub fn set_validator_id(
    consensus_manager_config: &mut ConsensusManagerConfig,
    node_index: usize,
) -> ValidatorId {
    let validator_id = ValidatorId::try_from(
        Felt::from(consensus_manager_config.consensus_manager_config.dynamic_config.validator_id)
            + Felt::from(node_index),
    )
    .unwrap();
    consensus_manager_config.consensus_manager_config.dynamic_config.validator_id = validator_id;
    validator_id
}

pub fn create_state_sync_configs(
    state_sync_storage_config: StorageConfig,
    ports: Vec<u16>,
    mut rpc_ports: Vec<u16>,
) -> Vec<StateSyncConfig> {
    create_connected_network_configs(ports)
        .into_iter()
        .map(|network_config| {
            let static_config = StateSyncStaticConfig {
                storage_config: state_sync_storage_config.clone(),
                network_config: Some(network_config),
                rpc_config: RpcConfig {
                    ip: Ipv4Addr::LOCALHOST.into(),
                    port: rpc_ports.remove(0),
                    ..Default::default()
                },
                ..Default::default()
            };
            StateSyncConfig { static_config, dynamic_config: StateSyncDynamicConfig::default() }
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

pub struct EndToEndTestScenario {
    pub create_rpc_txs_fn: CreateRpcTxsFn,
    pub create_l1_to_l2_messages_args_fn: CreateL1ToL2MessagesArgsFn,
    // TODO(Arni): replace with an optional apply shuffle to the tx hashes + a length assertion
    // parameter.
    pub test_tx_hashes_fn: TestTxHashesFn,
}

pub struct EndToEndFlowArgs {
    pub test_identifier: TestIdentifier,
    pub instance_indices: [u16; 3],
    pub test_scenario: EndToEndTestScenario,
    pub block_max_capacity_gas: GasAmount, // Used to max both sierra and proving gas.
    pub expecting_full_blocks: bool,
    pub expecting_reverted_transactions: bool,
    pub allow_bootstrap_txs: bool,
}

impl EndToEndFlowArgs {
    pub fn new(
        test_identifier: TestIdentifier,
        test_scenario: EndToEndTestScenario,
        block_max_capacity_gas: GasAmount,
    ) -> Self {
        Self {
            test_identifier,
            instance_indices: [0, 1, 2],
            test_scenario,
            block_max_capacity_gas,
            expecting_full_blocks: false,
            expecting_reverted_transactions: false,
            allow_bootstrap_txs: false,
        }
    }

    pub fn expecting_full_blocks(self) -> Self {
        Self { expecting_full_blocks: true, ..self }
    }

    pub fn expecting_reverted_transactions(self) -> Self {
        Self { expecting_reverted_transactions: true, ..self }
    }

    pub fn allow_bootstrap_txs(self) -> Self {
        Self { allow_bootstrap_txs: true, ..self }
    }

    pub fn instance_indices(self, instance_indices: [u16; 3]) -> Self {
        Self { instance_indices, ..self }
    }
}

// Note: run integration/flow tests from separate files in `tests/`, which helps cargo ensure
// isolation (prevent cross-contamination of services/resources) and that these tests won't be
// parallelized (which won't work with fixed ports).
pub async fn end_to_end_flow(args: EndToEndFlowArgs) {
    let EndToEndFlowArgs {
        test_identifier,
        instance_indices,
        test_scenario,
        block_max_capacity_gas,
        expecting_full_blocks,
        expecting_reverted_transactions,
        allow_bootstrap_txs,
    } = args;
    configure_tracing().await;

    let mut tx_generator = create_flow_test_tx_generator();
    let global_recorder_handle = PrometheusBuilder::new()
        .install_recorder()
        .expect("Should be able to install global prometheus recorder");

    const TEST_SCENARIO_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(50);
    // Setup.
    let mock_running_system = FlowTestSetup::new_from_tx_generator(
        &tx_generator,
        test_identifier.into(),
        block_max_capacity_gas,
        allow_bootstrap_txs,
        instance_indices,
    )
    .await;

    tokio::join!(
        wait_for_sequencer_node(&mock_running_system.sequencer_0),
        wait_for_sequencer_node(&mock_running_system.sequencer_1),
    );

    let sequencers = [&mock_running_system.sequencer_0, &mock_running_system.sequencer_1];
    // We use only the first sequencer's gateway to test that the mempools are syncing.
    let sequencer_to_add_txs = *sequencers.first().unwrap();
    let mut expected_proposer_iter = sequencers.iter().cycle();
    // We start at height 1, so we need to skip the proposer of the initial height.
    expected_proposer_iter.next().unwrap();
    let chain_id = mock_running_system.chain_id().clone();
    let mut send_rpc_tx_fn = |tx| sequencer_to_add_txs.assert_add_tx_success(tx);

    // In this test each sequencer increases the BATCHED_TRANSACTIONS metric which tracks the number
    // of accepted transactions. This tracks the cumulative count across all sequencers and
    // scenarios.
    let mut total_expected_batched_txs_count = 0;

    // Build multiple heights to ensure heights are committed.
    let EndToEndTestScenario {
        create_rpc_txs_fn,
        create_l1_to_l2_messages_args_fn,
        test_tx_hashes_fn,
    } = test_scenario;

    // Create and send transactions.
    // TODO(Arni): move send messages to l2 into [run_test_scenario].
    let l1_handlers = create_l1_to_l2_messages_args_fn(&mut tx_generator);
    mock_running_system.send_messages_to_l2(&l1_handlers).await;

    // Run the test scenario and get the expected batched tx hashes of the current scenario.
    let expected_batched_tx_hashes = run_test_scenario(
        &mut tx_generator,
        create_rpc_txs_fn,
        l1_handlers,
        &mut send_rpc_tx_fn,
        test_tx_hashes_fn,
        &chain_id,
    )
    .await;

    // Each sequencer increases the same BATCHED_TRANSACTIONS metric because they are running
    // in the same process in this test.
    total_expected_batched_txs_count += NUM_OF_SEQUENCERS * expected_batched_tx_hashes.len();
    let mut current_batched_txs_count = 0;

    tokio::time::timeout(TEST_SCENARIO_TIMEOUT, async {
        loop {
            info!(
                "Waiting for more txs to be batched in a block. Expected batched txs: \
                 {total_expected_batched_txs_count}, Currently batched txs: \
                 {current_batched_txs_count}"
            );

            current_batched_txs_count = get_total_batched_txs_count(&global_recorder_handle);
            if current_batched_txs_count == total_expected_batched_txs_count {
                break;
            }

            tokio::time::sleep(Duration::from_millis(2000)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!(
            "Expected transactions should be included in a block by now, Expected amount of \
             batched txs: {total_expected_batched_txs_count}, Currently amount of batched txs: \
             {current_batched_txs_count}"
        )
    });

    assert_full_blocks_flow(&global_recorder_handle, expecting_full_blocks);
    assert_on_number_of_reverted_transactions_flow(
        &global_recorder_handle,
        expecting_reverted_transactions,
    );
}

fn get_total_batched_txs_count(recorder_handle: &PrometheusHandle) -> usize {
    let metrics = recorder_handle.render();
    apollo_batcher::metrics::BATCHED_TRANSACTIONS.parse_numeric_metric::<usize>(&metrics).unwrap()
}

fn assert_full_blocks_flow(recorder_handle: &PrometheusHandle, expecting_full_blocks: bool) {
    if expecting_full_blocks {
        let metrics = recorder_handle.render();
        let full_blocks_metric = apollo_batcher::metrics::BLOCK_CLOSE_REASON
            .parse_numeric_metric::<u64>(
                &metrics,
                &[(
                    apollo_batcher::metrics::LABEL_NAME_BLOCK_CLOSE_REASON,
                    apollo_batcher::metrics::BlockCloseReason::FullBlock.into(),
                )],
            )
            .unwrap();
        assert!(
            full_blocks_metric > 0,
            "Expected full blocks, but found {full_blocks_metric} full blocks."
        );
    }
    // Just because we don't expect full blocks, doesn't mean we should assert that the metric is 0.
    // It is possible that a block is filled, no need to assert that this is not the case.
    // TODO(Arni): In the `else` case, assert that some block closed due to time.
}

fn assert_on_number_of_reverted_transactions_flow(
    recorder_handle: &PrometheusHandle,
    expecting_reverted_transactions: bool,
) {
    let metrics = recorder_handle.render();
    let reverted_transactions_metric =
        REVERTED_TRANSACTIONS.parse_numeric_metric::<u64>(&metrics).unwrap();

    if expecting_reverted_transactions {
        assert!(
            reverted_transactions_metric > 0,
            "Expected reverted transactions, but found {reverted_transactions_metric} reverted \
             transactions."
        );
    } else {
        assert_eq!(
            reverted_transactions_metric, 0,
            "Expected no reverted transactions, but found {reverted_transactions_metric} reverted \
             transactions."
        );
    }
}

async fn wait_for_sequencer_node(sequencer: &FlowSequencerSetup) {
    sequencer.monitoring_client.await_alive(5000, 50).await.expect("Node should be alive.");
}

pub fn test_single_tx(tx_hashes: &[TransactionHash]) -> Vec<TransactionHash> {
    validate_tx_count(tx_hashes, 1)
}

#[track_caller]
pub fn validate_tx_count(
    tx_hashes: &[TransactionHash],
    expected_count: usize,
) -> Vec<TransactionHash> {
    let tx_hashes_len = tx_hashes.len();
    assert_eq!(
        tx_hashes_len, expected_count,
        "Expected {expected_count} txs, but found {tx_hashes_len} txs.",
    );
    tx_hashes.to_vec()
}
