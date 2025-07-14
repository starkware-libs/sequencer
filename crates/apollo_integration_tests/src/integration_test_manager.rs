use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::net::{Ipv4Addr, SocketAddr};
use std::panic;
use std::path::PathBuf;
use std::time::Duration;

use alloy::node_bindings::AnvilInstance;
use apollo_http_server::config::HttpServerConfig;
use apollo_http_server::test_utils::HttpTestClient;
use apollo_infra_utils::dumping::serialize_to_file;
use apollo_infra_utils::test_utils::{AvailablePortsGenerator, TestIdentifier};
use apollo_infra_utils::tracing::{CustomLogger, TraceLevel};
use apollo_monitoring_endpoint::config::MonitoringEndpointConfig;
use apollo_monitoring_endpoint::test_utils::MonitoringClient;
use apollo_network::network_manager::test_utils::create_connected_network_configs;
use apollo_node::config::component_config::ComponentConfig;
use apollo_node::config::config_utils::DeploymentBaseAppConfig;
use apollo_node::config::definitions::ConfigPointersMap;
use apollo_node::config::node_config::{SequencerNodeConfig, CONFIG_NON_POINTERS_WHITELIST};
use apollo_node::test_utils::node_runner::{get_node_executable_path, spawn_run_node};
use apollo_storage::StorageConfig;
use apollo_test_utils::send_request;
use blockifier::context::ChainInfo;
use futures::future::join_all;
use futures::TryFutureExt;
use mempool_test_utils::starknet_api_test_utils::{
    contract_class,
    AccountId,
    MultiAccountTransactionGenerator,
};
use papyrus_base_layer::ethereum_base_layer_contract::StarknetL1Contract;
use papyrus_base_layer::test_utils::{
    ethereum_base_layer_config_for_anvil,
    make_block_history_on_anvil,
    spawn_anvil_and_deploy_starknet_l1_contract,
    DEFAULT_ANVIL_ADDITIONAL_ADDRESS_INDEX,
};
use starknet_api::block::BlockNumber;
use starknet_api::core::{ChainId, Nonce};
use starknet_api::execution_resources::GasAmount;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::state::SierraContractClass;
use starknet_api::transaction::TransactionHash;
use tokio::join;
use tokio_util::task::AbortOnDropHandle;
use tracing::{info, instrument};

use crate::executable_setup::{ExecutableSetup, NodeExecutionId};
use crate::monitoring_utils::{
    await_batcher_block,
    await_block,
    await_sync_block,
    await_txs_accepted,
    sequencer_num_accepted_txs,
    verify_txs_accepted,
};
use crate::node_component_configs::{
    create_consolidated_component_configs,
    create_distributed_component_configs,
    create_hybrid_component_configs,
};
use crate::sequencer_simulator_utils::SequencerSimulator;
use crate::state_reader::StorageTestHandles;
use crate::storage::{get_integration_test_storage, CustomPaths};
use crate::utils::{
    create_consensus_manager_configs_from_network_configs,
    create_integration_test_tx_generator,
    create_mempool_p2p_configs,
    create_node_config,
    create_state_sync_configs,
    send_consensus_txs,
    send_message_to_l2_and_calculate_tx_hash,
    set_validator_id,
    spawn_local_eth_to_strk_oracle,
    spawn_local_success_recorder,
    ConsensusTxs,
    DeclareTx,
    DeployAndInvokeTxs,
    TestScenario,
};

pub const DEFAULT_SENDER_ACCOUNT: AccountId = 0;
const BLOCK_MAX_CAPACITY_N_STEPS: GasAmount = GasAmount(30000000);
pub const BLOCK_TO_WAIT_FOR_DEPLOY_AND_INVOKE: BlockNumber = BlockNumber(4);
pub const BLOCK_TO_WAIT_FOR_DECLARE: BlockNumber =
    BlockNumber(BLOCK_TO_WAIT_FOR_DEPLOY_AND_INVOKE.0 + 10);

pub const HTTP_PORT_ARG: &str = "http-port";
pub const MONITORING_PORT_ARG: &str = "monitoring-port";

pub struct NodeSetup {
    executables: Vec<ExecutableSetup>,
    batcher_index: usize,
    http_server_index: usize,
    state_sync_index: usize,

    // Client for adding transactions to the sequencer node.
    pub add_tx_http_client: HttpTestClient,

    // Handles for the storage files, maintained so the files are not deleted. Since
    // these are only maintained to avoid dropping the handles, private visibility suffices, and
    // as such, the '#[allow(dead_code)]' attributes are used to suppress the warning.
    #[allow(dead_code)]
    storage_handles: StorageTestHandles,
}

// TODO(Nadin): reduce the number of arguments.
#[allow(clippy::too_many_arguments)]
impl NodeSetup {
    pub fn new(
        executables: Vec<ExecutableSetup>,
        batcher_index: usize,
        http_server_index: usize,
        state_sync_index: usize,
        add_tx_http_client: HttpTestClient,
        storage_handles: StorageTestHandles,
    ) -> Self {
        let len = executables.len();

        fn validate_index(index: usize, len: usize, label: &str) {
            assert!(
                index < len,
                "{} index {} is out of range. There are {} executables.",
                label,
                index,
                len
            );
        }

        validate_index(batcher_index, len, "Batcher");
        validate_index(http_server_index, len, "HTTP server");
        validate_index(state_sync_index, len, "State sync");

        Self {
            executables,
            batcher_index,
            http_server_index,
            state_sync_index,
            add_tx_http_client,
            storage_handles,
        }
    }

    async fn send_rpc_tx_fn(&self, rpc_tx: RpcTransaction) -> TransactionHash {
        self.add_tx_http_client.assert_add_tx_success(rpc_tx).await
    }

    pub fn batcher_monitoring_client(&self) -> &MonitoringClient {
        &self.executables[self.batcher_index].monitoring_client
    }

    pub fn state_sync_monitoring_client(&self) -> &MonitoringClient {
        &self.executables[self.state_sync_index].monitoring_client
    }

    pub fn get_executables(&self) -> &Vec<ExecutableSetup> {
        &self.executables
    }

    pub fn set_executable_config_path(
        &mut self,
        index: usize,
        new_path: PathBuf,
    ) -> Result<(), &'static str> {
        if let Some(exec) = self.executables.get_mut(index) {
            exec.node_config_path = new_path;
            Ok(())
        } else {
            panic!("Invalid executable index")
        }
    }

    pub fn generate_simulator_ports_json(&self, path: &str) {
        let json_data = serde_json::json!({
            HTTP_PORT_ARG: self.executables[self.http_server_index].get_config().http_server_config.port,
            MONITORING_PORT_ARG: self.executables[self.batcher_index].get_config().monitoring_endpoint_config.port
        });
        serialize_to_file(json_data, path);
    }

    pub fn get_batcher_index(&self) -> usize {
        self.batcher_index
    }

    pub fn get_http_server_index(&self) -> usize {
        self.http_server_index
    }

    pub fn get_state_sync_index(&self) -> usize {
        self.state_sync_index
    }

    pub fn run(self) -> RunningNode {
        let executable_handles = self
            .get_executables()
            .iter()
            .map(|executable| {
                info!("Running {}.", executable.node_execution_id);
                spawn_run_node(
                    vec![executable.node_config_path.clone()],
                    executable.node_execution_id.into(),
                )
            })
            .collect::<Vec<_>>();

        RunningNode { node_setup: self, executable_handles }
    }
    pub fn get_node_index(&self) -> Option<usize> {
        self.executables.first().map(|executable| executable.node_execution_id.get_node_index())
    }
}

pub struct RunningNode {
    node_setup: NodeSetup,
    executable_handles: Vec<AbortOnDropHandle<()>>,
}

impl RunningNode {
    async fn await_alive(&self, interval: u64, max_attempts: usize) {
        self.propagate_executable_panic();
        let await_alive_tasks = self.node_setup.executables.iter().map(|executable| {
            let result = executable.monitoring_client.await_alive(interval, max_attempts);
            result.unwrap_or_else(|_| {
                panic!("Executable {:?} should be alive.", executable.node_execution_id)
            })
        });

        join_all(await_alive_tasks).await;
    }

    fn propagate_executable_panic(&self) {
        for handle in &self.executable_handles {
            // A finished handle implies a running node executable has panicked.
            if handle.is_finished() {
                // Panic, dropping all other handles, which should drop.
                panic!("A running node executable has unexpectedly panicked.");
            }
        }
    }
}

pub struct IntegrationTestManager {
    node_indices: HashSet<usize>,
    idle_nodes: HashMap<usize, NodeSetup>,
    running_nodes: HashMap<usize, RunningNode>,
    tx_generator: MultiAccountTransactionGenerator,
    // Handle for L1 server: the server is dropped when handle is dropped.
    #[allow(dead_code)]
    l1_handle: AnvilInstance,
    starknet_l1_contract: StarknetL1Contract,
}

impl IntegrationTestManager {
    pub async fn new(
        num_of_consolidated_nodes: usize,
        num_of_distributed_nodes: usize,
        custom_paths: Option<CustomPaths>,
        test_unique_id: TestIdentifier,
    ) -> Self {
        let tx_generator = create_integration_test_tx_generator();

        let (sequencers_setup, node_indices) = get_sequencer_setup_configs(
            &tx_generator,
            num_of_consolidated_nodes,
            num_of_distributed_nodes,
            custom_paths,
            test_unique_id,
        )
        .await;

        let base_layer_config =
            &sequencers_setup[0].executables[0].base_app_config.get_config().base_layer_config;
        let (anvil, starknet_l1_contract) =
            spawn_anvil_and_deploy_starknet_l1_contract(base_layer_config).await;
        // Send some transactions to L1 so it has a history of blocks to scrape gas prices from.
        let num_blocks_needed_on_l1 = sequencers_setup[0].executables[0]
            .base_app_config
            .get_config()
            .l1_gas_price_scraper_config
            .shared
            .number_of_blocks_for_mean
            .try_into()
            .unwrap();
        let sender_address = anvil.addresses()[DEFAULT_ANVIL_ADDITIONAL_ADDRESS_INDEX];
        let receiver_address = anvil.addresses()[DEFAULT_ANVIL_ADDITIONAL_ADDRESS_INDEX + 1];

        make_block_history_on_anvil(
            sender_address,
            receiver_address,
            base_layer_config.clone(),
            num_blocks_needed_on_l1,
        )
        .await;

        let idle_nodes = create_map(sequencers_setup, |node| node.get_node_index());
        let running_nodes = HashMap::new();

        Self {
            node_indices,
            idle_nodes,
            running_nodes,
            tx_generator,
            l1_handle: anvil,
            starknet_l1_contract,
        }
    }

    pub fn get_idle_nodes(&self) -> &HashMap<usize, NodeSetup> {
        &self.idle_nodes
    }

    pub fn tx_generator(&self) -> &MultiAccountTransactionGenerator {
        &self.tx_generator
    }

    pub fn tx_generator_mut(&mut self) -> &mut MultiAccountTransactionGenerator {
        &mut self.tx_generator
    }

    pub async fn run_nodes(&mut self, nodes_to_run: HashSet<usize>) {
        info!("Checking that the sequencer node executable is present.");
        get_node_executable_path();
        // TODO(noamsp): Add size of nodes_to_run to the log.
        info!("Running specified nodes.");

        nodes_to_run.into_iter().for_each(|index| {
            let node_setup = self
                .idle_nodes
                .remove(&index)
                .unwrap_or_else(|| panic!("Node {} does not exist in idle_nodes.", index));
            info!("Running node {}.", index);
            let running_node = node_setup.run();
            assert!(
                self.running_nodes.insert(index, running_node).is_none(),
                "Node {} is already in the running map.",
                index
            );
        });

        // Wait for the nodes to start
        self.await_alive(5000, 50).await;
    }

    pub fn modify_config_idle_nodes<F>(
        &mut self,
        nodes_to_modify_config: HashSet<usize>,
        modify_config_fn: F,
    ) where
        F: Fn(&mut SequencerNodeConfig) + Copy,
    {
        info!("Modifying specified nodes config.");

        nodes_to_modify_config.into_iter().for_each(|node_index| {
            let node_setup = self
                .idle_nodes
                .get_mut(&node_index)
                .unwrap_or_else(|| panic!("Node {} does not exist in idle_nodes.", node_index));
            node_setup.executables.iter_mut().for_each(|executable| {
                info!("Modifying {} config.", executable.node_execution_id);
                executable.modify_config(modify_config_fn);
            });
        });
    }

    pub fn modify_config_pointers_idle_nodes<F>(
        &mut self,
        nodes_to_modify_config_pointers: HashSet<usize>,
        modify_config_pointers_fn: F,
    ) where
        F: Fn(&mut ConfigPointersMap) + Copy,
    {
        info!("Modifying specified nodes config pointers.");

        nodes_to_modify_config_pointers.into_iter().for_each(|node_index| {
            let node_setup = self
                .idle_nodes
                .get_mut(&node_index)
                .unwrap_or_else(|| panic!("Node {} does not exist in idle_nodes.", node_index));
            node_setup.executables.iter_mut().for_each(|executable| {
                info!("Modifying {} config pointers.", executable.node_execution_id);
                executable.modify_config_pointers(modify_config_pointers_fn);
            });
        });
    }

    pub async fn await_revert_all_running_nodes(
        &self,
        expected_block_number: BlockNumber,
        timeout_duration: Duration,
        interval_ms: u64,
        max_attempts: usize,
    ) {
        info!("Waiting for all idle nodes to finish reverting.");
        let condition =
            |&latest_block_number: &BlockNumber| latest_block_number == expected_block_number;

        let await_reverted_tasks = self.running_nodes.values().map(|running_node| async {
            let running_node_setup = &running_node.node_setup;
            let batcher_logger = CustomLogger::new(
                TraceLevel::Info,
                Some(format!(
                    "Waiting for batcher to reach block {expected_block_number} in sequencer {} \
                     executable {}.",
                    running_node_setup.get_node_index().unwrap(),
                    running_node_setup.get_batcher_index(),
                )),
            );

            // TODO(noamsp): rename batcher index/monitoringClient or use sync
            // index/monitoringClient
            let sync_logger = CustomLogger::new(
                TraceLevel::Info,
                Some(format!(
                    "Waiting for state sync to reach block {expected_block_number} in sequencer \
                     {} executable {}.",
                    running_node_setup.get_node_index().unwrap(),
                    running_node_setup.get_state_sync_index(),
                )),
            );

            join!(
                await_batcher_block(
                    interval_ms,
                    condition,
                    max_attempts,
                    running_node_setup.batcher_monitoring_client(),
                    batcher_logger,
                ),
                await_sync_block(
                    interval_ms,
                    condition,
                    max_attempts,
                    running_node_setup.state_sync_monitoring_client(),
                    sync_logger,
                )
            )
        });

        tokio::time::timeout(timeout_duration, join_all(await_reverted_tasks))
            .await
            .expect("Running Nodes should be reverted.");

        info!(
            "All running nodes have been reverted succesfully to block number \
             {expected_block_number}."
        );
    }

    pub fn get_node_indices(&self) -> HashSet<usize> {
        self.node_indices.clone()
    }

    pub fn shutdown_nodes(&mut self, nodes_to_shutdown: HashSet<usize>) {
        nodes_to_shutdown.into_iter().for_each(|index| {
            let running_node = self
                .running_nodes
                .remove(&index)
                .unwrap_or_else(|| panic!("Node {} is not in the running map.", index));
            running_node.executable_handles.iter().for_each(|handle| {
                assert!(!handle.is_finished(), "Node {} should still be running.", index);
                handle.abort();
            });
            assert!(
                self.idle_nodes.insert(index, running_node.node_setup).is_none(),
                "Node {} is already in the idle map.",
                index
            );
            info!("Node {} has been shut down.", index);
        });
    }

    pub async fn send_deploy_and_invoke_txs_and_verify(&mut self) {
        self.test_and_verify(
            DeployAndInvokeTxs,
            DEFAULT_SENDER_ACCOUNT,
            BLOCK_TO_WAIT_FOR_DEPLOY_AND_INVOKE,
        )
        .await;
    }

    #[instrument(skip(self))]
    pub async fn send_txs_and_verify(
        &mut self,
        n_invoke_txs: usize,
        n_l1_handler_txs: usize,
        wait_for_block: BlockNumber,
    ) {
        info!(
            "Sending {} invoke + {} l1handler txs and waiting for block {}.",
            n_invoke_txs, n_l1_handler_txs, wait_for_block
        );
        self.test_and_verify(
            ConsensusTxs { n_invoke_txs, n_l1_handler_txs },
            DEFAULT_SENDER_ACCOUNT,
            wait_for_block,
        )
        .await;
        self.rpc_verify_last_block(wait_for_block).await;
    }

    /// Create a simulator that's connected to the http server of Node 0.
    pub fn create_simulator(&self) -> SequencerSimulator {
        let node_0_setup = self
            .running_nodes
            .get(&0)
            .map(|node| &(node.node_setup))
            .unwrap_or_else(|| self.idle_nodes.get(&0).expect("Node 0 doesn't exist"));
        let config = node_0_setup
            .executables
            .get(node_0_setup.http_server_index)
            .expect("http_server_index points to a non existing executable index")
            .get_config();

        let localhost_url = format!("http://{}", Ipv4Addr::LOCALHOST);
        SequencerSimulator::new(
            localhost_url.clone(),
            config.http_server_config.port,
            localhost_url,
            config.monitoring_endpoint_config.port,
        )
    }

    #[instrument(skip(self))]
    pub async fn send_declare_txs_and_verify(&mut self) {
        info!("Sending a declare tx and waiting for block {}.", BLOCK_TO_WAIT_FOR_DECLARE);
        self.test_and_verify(DeclareTx, DEFAULT_SENDER_ACCOUNT, BLOCK_TO_WAIT_FOR_DECLARE).await;
        self.rpc_verify_class_declared(BLOCK_TO_WAIT_FOR_DECLARE).await;
    }

    pub async fn await_txs_accepted_on_all_running_nodes(&mut self, target_n_txs: usize) {
        self.perform_action_on_all_running_nodes(|sequencer_idx, running_node| {
            let monitoring_client = running_node.node_setup.state_sync_monitoring_client();
            await_txs_accepted(monitoring_client, sequencer_idx, target_n_txs)
        })
        .await;
    }

    /// This function tests and verifies the integration of the transaction flow.
    ///
    /// # Parameters
    /// - `expected_initial_value`: The initial amount of batched transactions. This represents the
    ///   starting state before any transactions are sent.
    /// - `n_txs`: The number of transactions that will be sent during the test. After the test
    ///   completes, the nonce in the batcher's storage is expected to be `expected_initial_value +
    ///   n_txs`.
    /// - `tx_generator`: A transaction generator used to create transactions for testing.
    /// - `sender_account`: The ID of the account sending the transactions.
    /// - `expected_block_number`: The block number up to which execution should be awaited.
    ///
    /// The function verifies the initial state, runs the test with the given number of
    /// transactions, waits for execution to complete, and then verifies the final state.
    async fn test_and_verify(
        &mut self,
        test_scenario: impl TestScenario,
        sender_account: AccountId,
        wait_for_block: BlockNumber,
    ) {
        self.verify_txs_accepted_on_all_running_nodes(sender_account).await;
        self.run_integration_test_simulator(&test_scenario, sender_account).await;
        self.await_block_on_all_running_nodes(wait_for_block).await;
        self.verify_txs_accepted_on_all_running_nodes(sender_account).await;
    }

    async fn await_alive(&self, interval: u64, max_attempts: usize) {
        let await_alive_tasks =
            self.running_nodes.values().map(|node| node.await_alive(interval, max_attempts));

        join_all(await_alive_tasks).await;
    }

    // Get RPC server socket address for the node 0.
    fn get_rpc_server_socket(&self) -> SocketAddr {
        let node_0_setup = self
            .running_nodes
            .get(&0)
            .map(|node| &(node.node_setup))
            .unwrap_or_else(|| self.idle_nodes.get(&0).expect("Node 0 doesn't exist"));
        let config = node_0_setup
            .executables
            .get(node_0_setup.http_server_index)
            .expect("http_server_index points to a non existing executable index")
            .get_config();

        SocketAddr::from((
            config.state_sync_config.rpc_config.ip,
            config.state_sync_config.rpc_config.port,
        ))
    }

    // Verify with JSON RPC server if the last block is the expected one.
    async fn rpc_verify_last_block(&self, expected_block: BlockNumber) {
        info!("Verifying last block number by JSON RPC server.");

        let server_address = self.get_rpc_server_socket();
        let res = send_request(server_address, "starknet_blockNumber", "", "V0_8").await;
        if let Some(block_number) = res.get("result").and_then(|result| result.as_u64()) {
            assert!(
                block_number >= expected_block.0,
                "JSON RPC server -> Block number mismatch: expected greater or equal than {}, got \
                 {}.",
                expected_block.0,
                block_number
            );
        } else {
            info!("JSON RPC server -> Received: {:?}", res);
            panic!(
                "JSON RPC server -> Failed to extract block number: 'result' field is missing or \
                 not a valid u64."
            );
        }
    }

    // Verify with JSON RPC server that class is declare transaction was successful.
    async fn rpc_verify_class_declared(&self, expected_block: BlockNumber) {
        info!("Verifying class declaration by JSON RPC server.");

        let server_address = self.get_rpc_server_socket();
        let declared_contract_class = contract_class();
        let class_hash = declared_contract_class.calculate_class_hash();
        let params = format!(
            r#"{{"block_number": {}}}, "0x{}""#,
            expected_block,
            hex::encode(class_hash.0.to_bytes_be())
        );

        info!(
            "rpc_verify_class_declared: server_addrerss: {}, class_version: {},  params {}",
            server_address, declared_contract_class.contract_class_version, params
        );

        let res = send_request(server_address, "starknet_getClass", params.as_str(), "V0_8").await;
        if let Some(received_contract_class) = res.get("result") {
            let contract_class: SierraContractClass =
                serde_json::from_value(received_contract_class.clone())
                    .expect("Failed to convert received contract class to SierraContractClass");
            assert_eq!(
                contract_class, declared_contract_class,
                "JSON RPC server -> Contract Class mismatch.",
            );
        } else {
            panic!("JSON RPC server -> Failed to get class");
        }
    }

    async fn run_integration_test_simulator(
        &mut self,
        test_scenario: &impl TestScenario,
        sender_account: AccountId,
    ) {
        info!("Running integration test simulator.");
        let chain_id = self.chain_id();
        let send_l1_handler_tx_fn = &mut |l1_handler_tx| {
            send_message_to_l2_and_calculate_tx_hash(
                l1_handler_tx,
                &self.starknet_l1_contract,
                &chain_id,
            )
        };
        let send_rpc_tx_fn = &mut |rpc_tx| async {
            let node_0 = self.running_nodes.get(&0).expect("Node 0 should be running.");
            node_0.node_setup.send_rpc_tx_fn(rpc_tx).await
        };

        send_consensus_txs(
            &mut self.tx_generator,
            sender_account,
            test_scenario,
            send_rpc_tx_fn,
            send_l1_handler_tx_fn,
        )
        .await;
    }

    /// Waits until all running nodes reach the specified block number.
    /// Queries the batcher and state sync metrics to verify progress.
    async fn await_block_on_all_running_nodes(&self, expected_block_number: BlockNumber) {
        self.perform_action_on_all_running_nodes(|sequencer_idx, running_node| {
            let node_setup = &running_node.node_setup;
            let batcher_monitoring_client = node_setup.batcher_monitoring_client();
            let batcher_index = node_setup.get_batcher_index();
            let state_sync_monitoring_client = node_setup.state_sync_monitoring_client();
            let state_sync_index = node_setup.get_state_sync_index();
            await_block(
                batcher_monitoring_client,
                batcher_index,
                state_sync_monitoring_client,
                state_sync_index,
                expected_block_number,
                sequencer_idx,
            )
        })
        .await;
    }

    pub async fn await_sync_block_on_all_running_nodes(
        &mut self,
        expected_block_number: BlockNumber,
    ) {
        let condition =
            |&latest_block_number: &BlockNumber| latest_block_number >= expected_block_number;

        self.perform_action_on_all_running_nodes(|sequencer_idx, running_node| async move {
            let node_setup = &running_node.node_setup;
            let monitoring_client = node_setup.batcher_monitoring_client();
            let batcher_index = node_setup.get_batcher_index();
            let expected_height = expected_block_number.unchecked_next();

            let logger = CustomLogger::new(
                TraceLevel::Info,
                Some(format!(
                    "Waiting for sync height metric to reach block {expected_height} in sequencer \
                     {sequencer_idx} executable {batcher_index}.",
                )),
            );
            await_sync_block(5000, condition, 50, monitoring_client, logger).await.unwrap();
        })
        .await;
    }

    async fn verify_txs_accepted_on_all_running_nodes(&self, sender_account: AccountId) {
        // We use state syncs processed txs metric via its monitoring client to verify that the
        // transactions were accepted.
        let account = self.tx_generator.account_with_id(sender_account);
        let expected_n_accepted_account_txs = nonce_to_usize(account.get_nonce());
        let expected_n_l1_handler_txs = self.tx_generator.n_l1_txs();
        let expected_n_accepted_txs = expected_n_accepted_account_txs + expected_n_l1_handler_txs;

        self.perform_action_on_all_running_nodes(|sequencer_idx, running_node| {
            // We use state syncs processed txs metric via its monitoring client to verify that the
            // transactions were accepted.
            let monitoring_client = running_node.node_setup.state_sync_monitoring_client();
            verify_txs_accepted(monitoring_client, sequencer_idx, expected_n_accepted_txs)
        })
        .await;
    }

    async fn perform_action_on_all_running_nodes<'a, F, Fut>(&'a self, f: F)
    where
        F: Fn(usize, &'a RunningNode) -> Fut,
        Fut: Future<Output = ()> + 'a,
    {
        let futures = self.running_nodes.iter().map(|(sequencer_idx, running_node)| {
            running_node.propagate_executable_panic();
            f(*sequencer_idx, running_node)
        });
        join_all(futures).await;
    }

    pub fn chain_id(&self) -> ChainId {
        // TODO(Arni): Get the chain ID from a shared canonic location.
        let node_setup = self
            .idle_nodes
            .values()
            .next()
            .or_else(|| self.running_nodes.values().next().map(|node| &node.node_setup))
            .expect("There should be at least one running or idle node");

        node_setup.executables[0]
            .get_config()
            .batcher_config
            .block_builder_config
            .chain_info
            .chain_id
            .clone()
    }

    /// This function returns the number of accepted transactions on all running nodes.
    /// It queries the state sync monitoring client to get the latest value of the processed txs
    /// metric.
    pub async fn get_num_accepted_txs_on_all_running_nodes(&self) -> HashMap<usize, usize> {
        let mut result = HashMap::new();
        for (index, running_node) in self.running_nodes.iter() {
            let monitoring_client = running_node.node_setup.state_sync_monitoring_client();
            let num_accepted = sequencer_num_accepted_txs(monitoring_client).await;
            result.insert(*index, num_accepted);
        }
        result
    }
}

pub fn nonce_to_usize(nonce: Nonce) -> usize {
    let prefixed_hex = nonce.0.to_hex_string();
    let unprefixed_hex = prefixed_hex.split_once("0x").unwrap().1;
    usize::from_str_radix(unprefixed_hex, 16).unwrap()
}

pub async fn get_sequencer_setup_configs(
    tx_generator: &MultiAccountTransactionGenerator,
    // TODO(Tsabary/Nadin): instead of number of nodes, this should be a vector of deployments.
    num_of_consolidated_nodes: usize,
    num_of_distributed_nodes: usize,
    custom_paths: Option<CustomPaths>,
    test_unique_id: TestIdentifier,
) -> (Vec<NodeSetup>, HashSet<usize>) {
    let mut available_ports_generator = AvailablePortsGenerator::new(test_unique_id.into());

    let mut node_component_configs =
        Vec::with_capacity(num_of_consolidated_nodes + num_of_distributed_nodes);
    for _ in 0..num_of_consolidated_nodes {
        node_component_configs.push(create_consolidated_component_configs());
    }
    // Testing the two various node configurations: distributed and hybrid.
    // TODO(Tsabary): better handling of the number of each type.
    for _ in 0..num_of_distributed_nodes / 2 {
        node_component_configs
            .push(create_hybrid_component_configs(&mut available_ports_generator));
    }
    for _ in num_of_distributed_nodes / 2..num_of_distributed_nodes {
        node_component_configs
            .push(create_distributed_component_configs(&mut available_ports_generator));
    }

    info!("Creating node configurations.");
    let chain_info = ChainInfo::create_for_testing();
    let accounts = tx_generator.accounts();
    let component_configs_len = node_component_configs.len();

    // TODO(Nadin): Refactor to avoid directly mutating vectors

    let mut consensus_manager_ports = available_ports_generator
        .next()
        .expect("Failed to get an AvailablePorts instance for consensus manager configs");

    // TODO(Nadin): pass recorder_url to this function to avoid mutating the resulting configs.
    let mut consensus_manager_configs = create_consensus_manager_configs_from_network_configs(
        create_connected_network_configs(
            consensus_manager_ports.get_next_ports(component_configs_len),
        ),
        component_configs_len,
        &chain_info.chain_id,
    );

    let node_indices: HashSet<usize> = (0..component_configs_len).collect();

    // TODO(Nadin): define the test storage here and pass it to the create_state_sync_configs and to
    // the ExecutableSetup
    let mut state_sync_ports = available_ports_generator
        .next()
        .expect("Failed to get an AvailablePorts instance for state sync configs");
    let mut state_sync_configs = create_state_sync_configs(
        StorageConfig::default(),
        state_sync_ports.get_next_ports(component_configs_len),
        state_sync_ports.get_next_ports(component_configs_len),
    );

    let mut mempool_p2p_ports = available_ports_generator
        .next()
        .expect("Failed to get an AvailablePorts instance for mempool p2p configs");
    let mut mempool_p2p_configs = create_mempool_p2p_configs(
        chain_info.chain_id.clone(),
        mempool_p2p_ports.get_next_ports(component_configs_len),
    );

    let mut base_layer_ports = available_ports_generator
        .next()
        .expect("Failed to get an AvailablePorts instance for base layer config");
    let base_layer_config =
        ethereum_base_layer_config_for_anvil(Some(base_layer_ports.get_next_port()));

    let mut nodes = Vec::new();

    // All nodes use the same recorder_url and eth_to_strk_oracle_url.
    let (recorder_url, _join_handle) =
        spawn_local_success_recorder(base_layer_ports.get_next_port());
    let (eth_to_strk_oracle_url, _join_handle_eth_to_strk_oracle) =
        spawn_local_eth_to_strk_oracle(base_layer_ports.get_next_port());

    let mut config_available_ports = available_ports_generator
        .next()
        .expect("Failed to get an AvailablePorts instance for node configs");

    for (node_index, node_component_config) in node_component_configs.into_iter().enumerate() {
        let mut executables = Vec::new();
        let batcher_index = node_component_config.get_batcher_index();
        let http_server_index = node_component_config.get_http_server_index();
        let state_sync_index = node_component_config.get_state_sync_index();
        let class_manager_index = node_component_config.get_class_manager_index();

        let mut consensus_manager_config = consensus_manager_configs.remove(0);
        let mempool_p2p_config = mempool_p2p_configs.remove(0);
        let state_sync_config = state_sync_configs.remove(0);

        consensus_manager_config.cende_config.recorder_url = recorder_url.clone();
        consensus_manager_config.eth_to_strk_oracle_config.base_url =
            eth_to_strk_oracle_url.clone();

        let validator_id = set_validator_id(&mut consensus_manager_config, node_index);
        let chain_info = chain_info.clone();

        let storage_setup = get_integration_test_storage(
            node_index,
            batcher_index,
            state_sync_index,
            class_manager_index,
            custom_paths.clone(),
            accounts.to_vec(),
            &chain_info,
        );

        // Derive the configuration for the sequencer node.
        let allow_bootstrap_txs = false;
        let (config, config_pointers_map) = create_node_config(
            &mut config_available_ports,
            chain_info,
            storage_setup.storage_config.clone(),
            state_sync_config,
            consensus_manager_config,
            mempool_p2p_config,
            MonitoringEndpointConfig::default(),
            ComponentConfig::default(),
            base_layer_config.clone(),
            BLOCK_MAX_CAPACITY_N_STEPS,
            validator_id,
            allow_bootstrap_txs,
        );
        let base_app_config = DeploymentBaseAppConfig::new(
            config.clone(),
            config_pointers_map.clone(),
            CONFIG_NON_POINTERS_WHITELIST.clone(),
        );

        let HttpServerConfig { ip, port } = config.http_server_config;
        let add_tx_http_client = HttpTestClient::new(SocketAddr::from((ip, port)));

        for (executable_index, executable_component_config) in
            node_component_config.into_iter().enumerate()
        {
            let node_execution_id = NodeExecutionId::new(node_index, executable_index);
            let exec_config_path =
                custom_paths.as_ref().and_then(|paths| paths.get_config_path(&node_execution_id));

            executables.push(
                ExecutableSetup::new(
                    base_app_config.clone(),
                    node_execution_id,
                    available_ports_generator
                        .next()
                        .expect("Failed to get an AvailablePorts instance for executable configs"),
                    exec_config_path,
                    executable_component_config,
                )
                .await,
            );
        }
        nodes.push(NodeSetup::new(
            executables,
            batcher_index,
            http_server_index,
            state_sync_index,
            add_tx_http_client,
            storage_setup.storage_handles,
        ));
    }

    (nodes, node_indices)
}

fn create_map<T, K, F>(items: Vec<T>, key_extractor: F) -> HashMap<K, T>
where
    F: Fn(&T) -> Option<K>,
    K: std::hash::Hash + Eq,
{
    items.into_iter().filter_map(|item| key_extractor(&item).map(|key| (key, item))).collect()
}
