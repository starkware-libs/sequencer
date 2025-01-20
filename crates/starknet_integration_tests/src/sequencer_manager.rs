use std::net::SocketAddr;

use futures::future::join_all;
use futures::TryFutureExt;
use mempool_test_utils::starknet_api_test_utils::{AccountId, MultiAccountTransactionGenerator};
use papyrus_execution::execution_utils::get_nonce_at;
use papyrus_network::network_manager::test_utils::create_connected_network_configs;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::{StorageConfig, StorageReader};
use starknet_api::block::BlockNumber;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::state::StateNumber;
use starknet_api::transaction::TransactionHash;
use starknet_infra_utils::run_until::run_until;
use starknet_infra_utils::test_utils::{
    AvailablePorts,
    TestIdentifier,
    MAX_NUMBER_OF_INSTANCES_PER_TEST,
};
use starknet_infra_utils::tracing::{CustomLogger, TraceLevel};
use starknet_sequencer_node::config::component_config::ComponentConfig;
use starknet_sequencer_node::config::component_execution_config::{
    ActiveComponentExecutionConfig,
    ReactiveComponentExecutionConfig,
};
use starknet_sequencer_node::test_utils::node_runner::spawn_run_node;
use starknet_types_core::felt::Felt;
use tokio::task::JoinHandle;
use tracing::info;

use crate::integration_test_setup::{ExecutableSetup, NodeExecutionId};
use crate::utils::{
    create_chain_info,
    create_consensus_manager_configs_from_network_configs,
    create_mempool_p2p_configs,
    create_state_sync_configs,
    send_account_txs,
};

/// The number of consolidated local sequencers that participate in the test.
const N_CONSOLIDATED_SEQUENCERS: usize = 3;
/// The number of distributed remote sequencers that participate in the test.
const N_DISTRIBUTED_SEQUENCERS: usize = 2;

/// Holds the component configs for a set of sequencers, composing a single sequencer node.
struct NodeComponentConfigs {
    component_configs: Vec<ComponentConfig>,
    batcher_index: usize,
    http_server_index: usize,
}

impl NodeComponentConfigs {
    fn new(
        component_configs: Vec<ComponentConfig>,
        batcher_index: usize,
        http_server_index: usize,
    ) -> Self {
        Self { component_configs, batcher_index, http_server_index }
    }

    fn into_iter(self) -> impl Iterator<Item = ComponentConfig> {
        self.component_configs.into_iter()
    }

    fn len(&self) -> usize {
        self.component_configs.len()
    }

    fn get_batcher_index(&self) -> usize {
        self.batcher_index
    }

    fn get_http_server_index(&self) -> usize {
        self.http_server_index
    }
}

pub struct NodeSetup {
    executables: Vec<ExecutableSetup>,
    batcher_index: usize,
    http_server_index: usize,
}

impl NodeSetup {
    pub fn new(
        executables: Vec<ExecutableSetup>,
        batcher_index: usize,
        http_server_index: usize,
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

        Self { executables, batcher_index, http_server_index }
    }

    async fn await_alive(&self, interval: u64, max_attempts: usize) {
        let await_alive_tasks = self.executables.iter().map(|executable| {
            let result = executable.monitoring_client.await_alive(interval, max_attempts);
            result.unwrap_or_else(|_| {
                panic!("Executable {:?} should be alive.", executable.node_execution_id)
            })
        });

        join_all(await_alive_tasks).await;
    }

    async fn send_rpc_tx_fn(&self, rpc_tx: RpcTransaction) -> TransactionHash {
        self.executables[self.http_server_index].assert_add_tx_success(rpc_tx).await
    }

    fn batcher_storage_reader(&self) -> StorageReader {
        let (batcher_storage_reader, _) = papyrus_storage::open_storage(
            self.executables[self.batcher_index].batcher_storage_config.clone(),
        )
        .expect("Failed to open batcher's storage");
        batcher_storage_reader
    }

    pub fn get_executables(&self) -> &Vec<ExecutableSetup> {
        &self.executables
    }

    pub fn get_batcher_index(&self) -> usize {
        self.batcher_index
    }

    pub fn get_http_server_index(&self) -> usize {
        self.http_server_index
    }
}

pub struct IntegrationTestManager {
    pub nodes: Vec<NodeSetup>,
    // TODO(Nadin): move to NodeSetup.
    pub executable_handles: Vec<JoinHandle<()>>,
}

impl IntegrationTestManager {
    pub async fn run(nodes: Vec<NodeSetup>) -> Self {
        info!("Running nodes.");
        let executable_handles = nodes
            .iter()
            .flat_map(|node| {
                node.get_executables().iter().map(|executable| {
                    spawn_run_node(
                        executable.node_config_path.clone(),
                        executable.node_execution_id.into(),
                    )
                })
            })
            .collect::<Vec<_>>();

        let sequencer_manager = Self { nodes, executable_handles };

        // Wait for the nodes to start.
        sequencer_manager.await_alive(5000, 50).await;

        sequencer_manager
    }

    async fn await_alive(&self, interval: u64, max_attempts: usize) {
        let await_alive_tasks =
            self.nodes.iter().map(|node| node.await_alive(interval, max_attempts));

        join_all(await_alive_tasks).await;
    }

    async fn send_rpc_tx_fn(&self, rpc_tx: RpcTransaction) -> TransactionHash {
        self.nodes[0].send_rpc_tx_fn(rpc_tx).await
    }

    fn batcher_storage_reader(&self) -> StorageReader {
        self.nodes[0].batcher_storage_reader()
    }

    pub fn shutdown_nodes(&self) {
        self.executable_handles.iter().for_each(|handle| {
            assert!(!handle.is_finished(), "Node should still be running.");
            handle.abort()
        });
    }

    pub async fn run_integration_test_simulator(
        &self,
        tx_generator: &mut MultiAccountTransactionGenerator,
        n_txs: usize,
        sender_account: AccountId,
    ) {
        info!("Running integration test simulator.");
        let send_rpc_tx_fn = &mut |rpc_tx| self.send_rpc_tx_fn(rpc_tx);

        info!("Sending {n_txs} txs.");
        let tx_hashes = send_account_txs(tx_generator, sender_account, n_txs, send_rpc_tx_fn).await;
        assert_eq!(tx_hashes.len(), n_txs);
    }

    pub async fn await_execution(&self, expected_block_number: BlockNumber) {
        info!("Awaiting until {expected_block_number} blocks have been created.");
        await_block(5000, expected_block_number, 50, &self.batcher_storage_reader())
            .await
            .expect("Block number should have been reached.");
    }

    pub async fn verify_results(&self, sender_address: ContractAddress, n_txs: usize) {
        info!("Verifying tx sender account nonce.");
        let expected_nonce_value = n_txs + 1;
        let expected_nonce =
            Nonce(Felt::from_hex_unchecked(format!("0x{:X}", expected_nonce_value).as_str()));
        let nonce = get_account_nonce(&self.batcher_storage_reader(), sender_address);
        assert_eq!(nonce, expected_nonce);
    }
}

/// Reads the latest block number from the storage.
fn get_latest_block_number(storage_reader: &StorageReader) -> BlockNumber {
    let txn = storage_reader.begin_ro_txn().unwrap();
    txn.get_state_marker()
        .expect("There should always be a state marker")
        .prev()
        .expect("There should be a previous block in the storage, set by the test setup")
}

/// Reads an account nonce after a block number from storage.
fn get_account_nonce(storage_reader: &StorageReader, contract_address: ContractAddress) -> Nonce {
    let block_number = get_latest_block_number(storage_reader);
    let txn = storage_reader.begin_ro_txn().unwrap();
    let state_number = StateNumber::unchecked_right_after_block(block_number);
    get_nonce_at(&txn, state_number, None, contract_address)
        .expect("Should always be Ok(Some(Nonce))")
        .expect("Should always be Some(Nonce)")
}

/// Sample a storage until sufficiently many blocks have been stored. Returns an error if after
/// the given number of attempts the target block number has not been reached.
async fn await_block(
    interval: u64,
    target_block_number: BlockNumber,
    max_attempts: usize,
    storage_reader: &StorageReader,
) -> Result<BlockNumber, ()> {
    let condition = |&latest_block_number: &BlockNumber| latest_block_number >= target_block_number;
    let get_latest_block_number_closure = || async move { get_latest_block_number(storage_reader) };

    let logger = CustomLogger::new(
        TraceLevel::Info,
        Some("Waiting for storage to include block".to_string()),
    );

    run_until(interval, max_attempts, get_latest_block_number_closure, condition, Some(logger))
        .await
        .ok_or(())
}

pub(crate) async fn get_sequencer_setup_configs(
    tx_generator: &MultiAccountTransactionGenerator,
) -> Vec<NodeSetup> {
    let test_unique_id = TestIdentifier::EndToEndIntegrationTest;

    // TODO(Nadin): Assign a dedicated set of available ports to each sequencer.
    let mut available_ports =
        AvailablePorts::new(test_unique_id.into(), MAX_NUMBER_OF_INSTANCES_PER_TEST - 1);

    let node_component_configs: Vec<NodeComponentConfigs> = {
        let mut combined = Vec::new();
        // Create elements in place.
        combined.extend(create_consolidated_sequencer_configs(N_CONSOLIDATED_SEQUENCERS));
        combined.extend(create_distributed_node_configs(
            &mut available_ports,
            N_DISTRIBUTED_SEQUENCERS,
        ));
        combined
    };

    info!("Creating node configurations.");
    let chain_info = create_chain_info();
    let accounts = tx_generator.accounts();
    let n_distributed_sequencers = node_component_configs
        .iter()
        .map(|node_component_config| node_component_config.len())
        .sum();

    // TODO (Nadin): Refactor to avoid directly mutating vectors

    let mut consensus_manager_configs = create_consensus_manager_configs_from_network_configs(
        create_connected_network_configs(available_ports.get_next_ports(n_distributed_sequencers)),
        node_component_configs.len(),
    );

    // TODO(Nadin): define the test storage here and pass it to the create_state_sync_configs and to
    // the ExecutableSetup
    let mut state_sync_configs = create_state_sync_configs(
        StorageConfig::default(),
        available_ports.get_next_ports(n_distributed_sequencers),
    );

    let mut mempool_p2p_configs = create_mempool_p2p_configs(
        chain_info.chain_id.clone(),
        available_ports.get_next_ports(n_distributed_sequencers),
    );

    // TODO(Nadin/Tsabary): There are redundant p2p configs here, as each distributed node
    // needs only one of them, but the current setup creates one per part. Need to refactor.

    let mut nodes = Vec::new();
    let mut global_index = 0;

    for (node_index, node_component_config) in node_component_configs.into_iter().enumerate() {
        let mut executables = Vec::new();
        let batcher_index = node_component_config.get_batcher_index();
        let http_server_index = node_component_config.get_http_server_index();

        for (executable_index, executable_component_config) in
            node_component_config.into_iter().enumerate()
        {
            let node_execution_id = NodeExecutionId::new(node_index, executable_index);
            let consensus_manager_config = consensus_manager_configs.remove(0);
            let mempool_p2p_config = mempool_p2p_configs.remove(0);
            let state_sync_config = state_sync_configs.remove(0);
            let chain_info = chain_info.clone();
            executables.push(
                ExecutableSetup::new(
                    accounts.to_vec(),
                    node_execution_id,
                    chain_info,
                    consensus_manager_config,
                    mempool_p2p_config,
                    state_sync_config,
                    AvailablePorts::new(test_unique_id.into(), global_index.try_into().unwrap()),
                    executable_component_config.clone(),
                )
                .await,
            );
            global_index += 1;
        }
        nodes.push(NodeSetup::new(executables, batcher_index, http_server_index));
    }

    nodes
}

/// Generates configurations for a specified number of distributed sequencer nodes,
/// each consisting of an HTTP component configuration and a non-HTTP component configuration.
/// returns a vector of vectors, where each inner vector contains the two configurations.
fn create_distributed_node_configs(
    available_ports: &mut AvailablePorts,
    distributed_sequencers_num: usize,
) -> Vec<NodeComponentConfigs> {
    std::iter::repeat_with(|| {
        let gateway_socket = available_ports.get_next_local_host_socket();
        let mempool_socket = available_ports.get_next_local_host_socket();
        let mempool_p2p_socket = available_ports.get_next_local_host_socket();
        let state_sync_socket = available_ports.get_next_local_host_socket();

        NodeComponentConfigs::new(
            vec![
                get_http_container_config(
                    gateway_socket,
                    mempool_socket,
                    mempool_p2p_socket,
                    state_sync_socket,
                ),
                get_non_http_container_config(
                    gateway_socket,
                    mempool_socket,
                    mempool_p2p_socket,
                    state_sync_socket,
                ),
            ],
            // batcher is in executable index 1.
            1,
            // http server is in executable index 0.
            0,
        )
    })
    .take(distributed_sequencers_num)
    .collect()
}

fn create_consolidated_sequencer_configs(
    num_of_consolidated_nodes: usize,
) -> Vec<NodeComponentConfigs> {
    // Both batcher and http server are in executable index 0.
    std::iter::repeat_with(|| NodeComponentConfigs::new(vec![ComponentConfig::default()], 0, 0))
        .take(num_of_consolidated_nodes)
        .collect()
}

// TODO(Nadin/Tsabary): find a better name for this function.
fn get_http_container_config(
    gateway_socket: SocketAddr,
    mempool_socket: SocketAddr,
    mempool_p2p_socket: SocketAddr,
    state_sync_socket: SocketAddr,
) -> ComponentConfig {
    let mut config = ComponentConfig::disabled();
    config.http_server = ActiveComponentExecutionConfig::default();
    config.gateway = ReactiveComponentExecutionConfig::local_with_remote_enabled(gateway_socket);
    config.mempool = ReactiveComponentExecutionConfig::local_with_remote_enabled(mempool_socket);
    config.mempool_p2p =
        ReactiveComponentExecutionConfig::local_with_remote_enabled(mempool_p2p_socket);
    config.state_sync = ReactiveComponentExecutionConfig::remote(state_sync_socket);
    config.monitoring_endpoint = ActiveComponentExecutionConfig::default();
    config
}

fn get_non_http_container_config(
    gateway_socket: SocketAddr,
    mempool_socket: SocketAddr,
    mempool_p2p_socket: SocketAddr,
    state_sync_socket: SocketAddr,
) -> ComponentConfig {
    ComponentConfig {
        http_server: ActiveComponentExecutionConfig::disabled(),
        monitoring_endpoint: Default::default(),
        gateway: ReactiveComponentExecutionConfig::remote(gateway_socket),
        mempool: ReactiveComponentExecutionConfig::remote(mempool_socket),
        mempool_p2p: ReactiveComponentExecutionConfig::remote(mempool_p2p_socket),
        state_sync: ReactiveComponentExecutionConfig::local_with_remote_enabled(state_sync_socket),
        ..ComponentConfig::default()
    }
}
