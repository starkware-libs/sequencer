use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::PathBuf;
use std::time::Duration;

use alloy::node_bindings::AnvilInstance;
use blockifier::context::ChainInfo;
use futures::future::join_all;
use futures::TryFutureExt;
use mempool_test_utils::starknet_api_test_utils::{AccountId, MultiAccountTransactionGenerator};
use papyrus_base_layer::test_utils::{
    ethereum_base_layer_config_for_anvil,
    spawn_anvil_and_deploy_starknet_l1_contract,
    StarknetL1Contract,
};
use papyrus_network::network_manager::test_utils::create_connected_network_configs;
use papyrus_storage::StorageConfig;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ChainId, Nonce};
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_infra_utils::test_utils::{AvailablePortsGenerator, TestIdentifier};
use starknet_infra_utils::tracing::{CustomLogger, TraceLevel};
use starknet_monitoring_endpoint::test_utils::MonitoringClient;
use starknet_sequencer_node::config::config_utils::dump_json_data;
use starknet_sequencer_node::config::node_config::SequencerNodeConfig;
use starknet_sequencer_node::test_utils::node_runner::{get_node_executable_path, spawn_run_node};
use tokio::join;
use tokio::task::JoinHandle;
use tracing::info;

use crate::integration_test_setup::{ConfigPointersMap, ExecutableSetup, NodeExecutionId};
use crate::monitoring_utils::{
    await_batcher_block,
    await_block,
    await_sync_block,
    await_txs_accepted,
    verify_txs_accepted,
};
use crate::node_component_configs::{
    create_consolidated_sequencer_configs,
    create_nodes_deployment_units_configs,
    NodeComponentConfigs,
};
use crate::utils::{
    create_consensus_manager_configs_from_network_configs,
    create_integration_test_tx_generator,
    create_mempool_p2p_configs,
    create_state_sync_configs,
    send_consensus_txs,
    send_message_to_l2_and_calculate_tx_hash,
    BootstrapTxs,
    ConsensusTxs,
    TestScenario,
};

const DEFAULT_SENDER_ACCOUNT: AccountId = 0;
pub const BLOCK_TO_WAIT_FOR_BOOTSTRAP: BlockNumber = BlockNumber(2);

pub const HTTP_PORT_ARG: &str = "http-port";
pub const MONITORING_PORT_ARG: &str = "monitoring-port";

pub struct NodeSetup {
    executables: Vec<ExecutableSetup>,
    batcher_index: usize,
    http_server_index: usize,
    state_sync_index: usize,
}

impl NodeSetup {
    pub fn new(
        executables: Vec<ExecutableSetup>,
        batcher_index: usize,
        http_server_index: usize,
        state_sync_index: usize,
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

        Self { executables, batcher_index, http_server_index, state_sync_index }
    }

    async fn send_rpc_tx_fn(&self, rpc_tx: RpcTransaction) -> TransactionHash {
        self.executables[self.http_server_index].assert_add_tx_success(rpc_tx).await
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
            HTTP_PORT_ARG: self.executables[self.http_server_index].config.http_server_config.port,
            MONITORING_PORT_ARG: self.executables[self.batcher_index].config.monitoring_endpoint_config.port
        });

        dump_json_data(json_data, &PathBuf::from(path));
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
                    executable.node_config_path.clone(),
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
    executable_handles: Vec<JoinHandle<()>>,
}

impl RunningNode {
    async fn await_alive(&self, interval: u64, max_attempts: usize) {
        let await_alive_tasks = self.node_setup.executables.iter().map(|executable| {
            let result = executable.monitoring_client.await_alive(interval, max_attempts);
            result.unwrap_or_else(|_| {
                panic!("Executable {:?} should be alive.", executable.node_execution_id)
            })
        });

        join_all(await_alive_tasks).await;
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

pub struct CustomPaths {
    db_base: Option<PathBuf>,
    config_base: Option<PathBuf>,
    data_prefix_base: Option<PathBuf>,
}

impl CustomPaths {
    pub fn new(
        db_base: Option<PathBuf>,
        config_base: Option<PathBuf>,
        data_prefix_base: Option<PathBuf>,
    ) -> Self {
        Self { db_base, config_base, data_prefix_base }
    }
    pub fn get_db_path(&self, node_execution_id: &NodeExecutionId) -> Option<PathBuf> {
        self.db_base.as_ref().map(|p| node_execution_id.build_path(p))
    }

    pub fn get_config_path(&self, node_execution_id: &NodeExecutionId) -> Option<PathBuf> {
        self.config_base.as_ref().map(|p| node_execution_id.build_path(p))
    }

    pub fn get_data_prefix_path(&self, node_execution_id: &NodeExecutionId) -> Option<PathBuf> {
        self.data_prefix_base.as_ref().map(|p| node_execution_id.build_path(p))
    }
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
            create_nodes_deployment_units_configs,
        )
        .await;

        let base_layer_config = &sequencers_setup[0].executables[0].config.base_layer_config;
        let (anvil, starknet_l1_contract) =
            spawn_anvil_and_deploy_starknet_l1_contract(base_layer_config).await;

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

    pub async fn send_bootstrap_txs_and_verify(&mut self) {
        self.test_and_verify(BootstrapTxs, DEFAULT_SENDER_ACCOUNT, BLOCK_TO_WAIT_FOR_BOOTSTRAP)
            .await;
    }

    pub async fn send_txs_and_verify(
        &mut self,
        n_invoke_txs: usize,
        n_l1_handler_txs: usize,
        wait_for_block: BlockNumber,
    ) {
        self.test_and_verify(
            ConsensusTxs { n_invoke_txs, n_l1_handler_txs },
            DEFAULT_SENDER_ACCOUNT,
            wait_for_block,
        )
        .await;
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

    // TODO(noamsp): Remove this once we make the function public and use it in the tests.
    #[allow(dead_code)]
    async fn await_sync_block_on_all_running_nodes(&mut self, expected_block_number: BlockNumber) {
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
        let futures = self
            .running_nodes
            .iter()
            .map(|(sequencer_idx, running_node)| f(*sequencer_idx, running_node));
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
            .config
            .batcher_config
            .block_builder_config
            .chain_info
            .chain_id
            .clone()
    }
}

pub fn nonce_to_usize(nonce: Nonce) -> usize {
    let prefixed_hex = nonce.0.to_hex_string();
    let unprefixed_hex = prefixed_hex.split_once("0x").unwrap().1;
    usize::from_str_radix(unprefixed_hex, 16).unwrap()
}

pub async fn get_sequencer_setup_configs(
    tx_generator: &MultiAccountTransactionGenerator,
    num_of_consolidated_nodes: usize,
    num_of_distributed_nodes: usize,
    custom_paths: Option<CustomPaths>,
    test_unique_id: TestIdentifier,
    distributed_configs_creation_function: fn(
        &mut AvailablePortsGenerator,
        usize,
    ) -> Vec<NodeComponentConfigs>,
) -> (Vec<NodeSetup>, HashSet<usize>) {
    let mut available_ports_generator = AvailablePortsGenerator::new(test_unique_id.into());

    let node_component_configs: Vec<NodeComponentConfigs> = {
        let mut combined = Vec::new();
        // Create elements in place.
        combined.extend(create_consolidated_sequencer_configs(num_of_consolidated_nodes));
        combined.extend(distributed_configs_creation_function(
            &mut available_ports_generator,
            num_of_distributed_nodes,
        ));
        combined
    };

    info!("Creating node configurations.");
    let chain_info = ChainInfo::create_for_testing();
    let accounts = tx_generator.accounts();
    let n_distributed_sequencers = node_component_configs
        .iter()
        .map(|node_component_config| node_component_config.len())
        .sum();

    // TODO(Nadin): Refactor to avoid directly mutating vectors

    let mut consensus_manager_ports = available_ports_generator
        .next()
        .expect("Failed to get an AvailablePorts instance for consensus manager configs");
    let mut consensus_manager_configs = create_consensus_manager_configs_from_network_configs(
        create_connected_network_configs(
            consensus_manager_ports.get_next_ports(n_distributed_sequencers),
        ),
        node_component_configs.len(),
        &chain_info.chain_id,
    );

    let node_indices: HashSet<usize> = (0..node_component_configs.len()).collect();

    // TODO(Nadin): define the test storage here and pass it to the create_state_sync_configs and to
    // the ExecutableSetup
    let mut state_sync_ports = available_ports_generator
        .next()
        .expect("Failed to get an AvailablePorts instance for state sync configs");
    let mut state_sync_configs = create_state_sync_configs(
        StorageConfig::default(),
        state_sync_ports.get_next_ports(n_distributed_sequencers),
    );

    let mut mempool_p2p_ports = available_ports_generator
        .next()
        .expect("Failed to get an AvailablePorts instance for mempool p2p configs");
    let mut mempool_p2p_configs = create_mempool_p2p_configs(
        chain_info.chain_id.clone(),
        mempool_p2p_ports.get_next_ports(n_distributed_sequencers),
    );

    let mut base_layer_ports = available_ports_generator
        .next()
        .expect("Failed to get an AvailablePorts instance for base layer config");
    let base_layer_config =
        ethereum_base_layer_config_for_anvil(Some(base_layer_ports.get_next_port()));

    // TODO(Nadin/Tsabary): There are redundant p2p configs here, as each distributed node
    // needs only one of them, but the current setup creates one per part. Need to refactor.

    let mut nodes = Vec::new();

    for (node_index, node_component_config) in node_component_configs.into_iter().enumerate() {
        let mut executables = Vec::new();
        let batcher_index = node_component_config.get_batcher_index();
        let http_server_index = node_component_config.get_http_server_index();
        let state_sync_index = node_component_config.get_state_sync_index();

        for (executable_index, executable_component_config) in
            node_component_config.into_iter().enumerate()
        {
            let node_execution_id = NodeExecutionId::new(node_index, executable_index);
            let consensus_manager_config = consensus_manager_configs.remove(0);
            let mempool_p2p_config = mempool_p2p_configs.remove(0);
            let state_sync_config = state_sync_configs.remove(0);
            let chain_info = chain_info.clone();
            let exec_db_path =
                custom_paths.as_ref().and_then(|paths| paths.get_db_path(&node_execution_id));
            let exec_config_path =
                custom_paths.as_ref().and_then(|paths| paths.get_config_path(&node_execution_id));
            let exec_data_prefix_dir = custom_paths
                .as_ref()
                .and_then(|paths| paths.get_data_prefix_path(&node_execution_id));

            executables.push(
                ExecutableSetup::new(
                    accounts.to_vec(),
                    node_execution_id,
                    chain_info,
                    consensus_manager_config,
                    mempool_p2p_config,
                    state_sync_config,
                    available_ports_generator
                        .next()
                        .expect("Failed to get an AvailablePorts instance for executable configs"),
                    executable_component_config.clone(),
                    base_layer_config.clone(),
                    exec_db_path,
                    exec_config_path,
                    exec_data_prefix_dir,
                )
                .await,
            );
        }
        nodes.push(NodeSetup::new(executables, batcher_index, http_server_index, state_sync_index));
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
