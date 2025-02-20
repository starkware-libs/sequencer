use std::future::Future;
use std::net::SocketAddr;
use std::time::Duration;

use axum::http::StatusCode;
use axum::routing::post;
use axum::Router;
use blockifier::blockifier::config::TransactionExecutorConfig;
use blockifier::bouncer::{BouncerConfig, BouncerWeights};
use blockifier::context::ChainInfo;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::FeatureContract;
use mempool_test_utils::starknet_api_test_utils::{
    AccountId,
    AccountTransactionGenerator,
    Contract,
    MultiAccountTransactionGenerator,
};
use papyrus_network::network_manager::test_utils::create_connected_network_configs;
use papyrus_network::NetworkConfig;
use papyrus_storage::StorageConfig;
use starknet_api::block::BlockNumber;
use starknet_api::core::ChainId;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::fields::ContractAddressSalt;
use starknet_api::transaction::TransactionHash;
use starknet_batcher::block_builder::BlockBuilderConfig;
use starknet_batcher::config::BatcherConfig;
use starknet_class_manager::class_storage::CachedClassStorageConfig;
use starknet_class_manager::config::{
    ClassManagerConfig,
    FsClassManagerConfig,
    FsClassStorageConfig,
};
use starknet_consensus::config::{ConsensusConfig, TimeoutsConfig};
use starknet_consensus::types::{ContextConfig, ValidatorId};
use starknet_consensus_manager::config::ConsensusManagerConfig;
use starknet_consensus_orchestrator::cende::{CendeConfig, RECORDER_WRITE_BLOB_PATH};
use starknet_gateway::config::{
    GatewayConfig,
    StatefulTransactionValidatorConfig,
    StatelessTransactionValidatorConfig,
};
use starknet_http_server::test_utils::create_http_server_config;
use starknet_infra_utils::test_utils::AvailablePorts;
use starknet_l1_provider::l1_scraper::L1ScraperConfig;
use starknet_mempool_p2p::config::MempoolP2pConfig;
use starknet_monitoring_endpoint::config::MonitoringEndpointConfig;
use starknet_sequencer_node::config::component_config::ComponentConfig;
use starknet_sequencer_node::config::config_utils::{
    EthereumBaseLayerConfigRequiredParams,
    RequiredParams,
};
use starknet_sequencer_node::config::node_config::SequencerNodeConfig;
use starknet_state_sync::config::StateSyncConfig;
use starknet_types_core::felt::Felt;
use tokio::task::JoinHandle;
use tracing::{debug, info, Instrument};
use url::Url;

use crate::integration_test_setup::NodeExecutionId;

pub const ACCOUNT_ID_0: AccountId = 0;
pub const ACCOUNT_ID_1: AccountId = 1;
pub const NEW_ACCOUNT_SALT: ContractAddressSalt = ContractAddressSalt(Felt::THREE);
pub const UNDEPLOYED_ACCOUNT_ID: AccountId = 2;
// Transactions per second sent to the gateway. This rate makes each block contain ~10 transactions
// with the set [TimeoutsConfig] .
pub const TPS: u64 = 2;
const N_TXS_IN_FIRST_BLOCK: usize = 2;

pub type CreateRpcTxsFn = fn(&mut MultiAccountTransactionGenerator) -> Vec<RpcTransaction>;
pub type TestTxHashesFn = fn(&[TransactionHash]) -> Vec<TransactionHash>;
pub type ExpectedContentId = Felt;

pub trait TestScenario {
    fn create_txs(
        &self,
        tx_generator: &mut MultiAccountTransactionGenerator,
        account_id: AccountId,
    ) -> Vec<RpcTransaction>;

    fn n_txs(&self) -> usize;
}

pub struct InvokeTxs(pub usize);

impl TestScenario for InvokeTxs {
    fn create_txs(
        &self,
        tx_generator: &mut MultiAccountTransactionGenerator,
        account_id: AccountId,
    ) -> Vec<RpcTransaction> {
        create_invoke_txs(tx_generator, account_id, self.0)
    }

    fn n_txs(&self) -> usize {
        self.0
    }
}

pub struct BootstrapTxs;

impl TestScenario for BootstrapTxs {
    fn create_txs(
        &self,
        tx_generator: &mut MultiAccountTransactionGenerator,
        account_id: AccountId,
    ) -> Vec<RpcTransaction> {
        let txs = create_deploy_account_tx_and_invoke_tx(tx_generator, account_id);
        assert_eq!(
            txs.len(),
            N_TXS_IN_FIRST_BLOCK,
            "First block should contain exactly {} transactions, but {} transactions were created",
            N_TXS_IN_FIRST_BLOCK,
            txs.len(),
        );
        txs
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
) -> (SequencerNodeConfig, RequiredParams) {
    let validator_id =
        set_validator_id(&mut consensus_manager_config, node_execution_id.get_node_index());
    let recorder_url = consensus_manager_config.cende_config.recorder_url.clone();
    let fee_token_addresses = chain_info.fee_token_addresses.clone();
    let batcher_config = create_batcher_config(batcher_storage_config, chain_info.clone());
    let gateway_config = create_gateway_config(chain_info.clone());
    let l1_scraper_config =
        L1ScraperConfig { chain_id: chain_info.chain_id.clone(), ..Default::default() };
    let http_server_config =
        create_http_server_config(available_ports.get_next_local_host_socket());
    let class_manager_config = create_class_manager_config(class_manager_storage_config);
    state_sync_config.storage_config = state_sync_storage_config;

    (
        SequencerNodeConfig {
            batcher_config,
            class_manager_config,
            consensus_manager_config,
            gateway_config,
            http_server_config,
            mempool_p2p_config,
            monitoring_endpoint_config,
            state_sync_config,
            components: component_config,
            l1_scraper_config,
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
                ..Default::default()
            },
            cende_config: CendeConfig{
                skip_write_height: Some(BlockNumber(1)),
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
pub async fn run_test_scenario<'a, Fut>(
    tx_generator: &mut MultiAccountTransactionGenerator,
    create_rpc_txs_fn: CreateRpcTxsFn,
    send_rpc_tx_fn: &'a mut dyn FnMut(RpcTransaction) -> Fut,
    test_tx_hashes_fn: TestTxHashesFn,
) -> Vec<TransactionHash>
where
    Fut: Future<Output = TransactionHash> + 'a,
{
    let rpc_txs = create_rpc_txs_fn(tx_generator);
    let tx_hashes = send_rpc_txs(rpc_txs, send_rpc_tx_fn).await;
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
pub async fn send_account_txs<'a, Fut>(
    tx_generator: &mut MultiAccountTransactionGenerator,
    account_id: AccountId,
    test_scenario: &impl TestScenario,
    send_rpc_tx_fn: &'a mut dyn FnMut(RpcTransaction) -> Fut,
) -> Vec<TransactionHash>
where
    Fut: Future<Output = TransactionHash> + 'a,
{
    let n_txs = test_scenario.n_txs();
    info!("Sending {n_txs} txs.");

    let rpc_txs = test_scenario.create_txs(tx_generator, account_id);
    let tx_hashes = send_rpc_txs(rpc_txs, send_rpc_tx_fn).await;
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
) -> BatcherConfig {
    // TODO(Arni): Create BlockBuilderConfig create for testing method and use here.
    let concurrency_enabled = true;
    BatcherConfig {
        storage: batcher_storage_config,
        block_builder_config: BlockBuilderConfig {
            chain_info,
            bouncer_config: BouncerConfig {
                block_max_capacity: BouncerWeights { n_steps: 75000, ..Default::default() },
            },
            execute_config: TransactionExecutorConfig::create_for_testing(concurrency_enabled),
            ..Default::default()
        },
        ..Default::default()
    }
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
