use std::net::SocketAddr;
use std::path::PathBuf;

use blockifier::context::ChainInfo;
use mempool_test_utils::starknet_api_test_utils::{Contract, MultiAccountTransactionGenerator};
use papyrus_storage::{StorageConfig, StorageReader};
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_consensus_manager::config::ConsensusManagerConfig;
use starknet_http_server::config::HttpServerConfig;
use starknet_http_server::test_utils::HttpTestClient;
use starknet_mempool_p2p::config::MempoolP2pConfig;
use starknet_monitoring_endpoint::config::MonitoringEndpointConfig;
use starknet_monitoring_endpoint::test_utils::IsAliveClient;
use starknet_sequencer_infra::test_utils::AvailablePorts;
use starknet_sequencer_node::config::component_config::ComponentConfig;
use starknet_sequencer_node::test_utils::node_runner::spawn_run_node;
use tempfile::{tempdir, TempDir};
use tokio::task::JoinHandle;
use tracing::{info, instrument};

use crate::config_utils::dump_config_file_changes;
use crate::state_reader::StorageTestSetup;
use crate::utils::{
    create_chain_info,
    create_consensus_manager_configs_and_channels,
    create_mempool_p2p_configs,
    create_node_config,
    spawn_success_recorder,
};

pub struct IntegrationTestSetup {
    pub sequencers: Vec<IntegrationSequencerSetup>,
    pub sequencer_run_handles: Vec<JoinHandle<()>>,
}

impl IntegrationTestSetup {
    pub async fn run(
        tx_generator: &MultiAccountTransactionGenerator,
        mut available_ports: AvailablePorts,
        component_configs: Vec<Vec<ComponentConfig>>,
    ) -> Self {
        let chain_info = create_chain_info();
        let accounts = tx_generator.accounts();
        let n_distributed_sequencers =
            component_configs.iter().map(|inner_vec| inner_vec.len()).sum();

        let (mut consensus_manager_configs, _) = create_consensus_manager_configs_and_channels(
            n_distributed_sequencers,
            &mut available_ports,
        );

        let ports = available_ports.get_next_ports(n_distributed_sequencers);
        let mut mempool_p2p_configs =
            create_mempool_p2p_configs(chain_info.chain_id.clone(), ports);

        let mut sequencers = vec![];
        for (sequencer_id, node_composition) in component_configs.iter().enumerate() {
            for component_config in node_composition {
                // Declare one consensus_manager_config and one mempool_p2p_config for each node
                // composition
                let consensus_manager_config = consensus_manager_configs.remove(0);
                let mempool_p2p_config = mempool_p2p_configs.remove(0);
                let sequencer = IntegrationSequencerSetup::new(
                    accounts.clone(),
                    sequencer_id,
                    chain_info.clone(),
                    consensus_manager_config,
                    mempool_p2p_config,
                    &mut available_ports,
                    component_config.clone(),
                )
                .await;
                sequencers.push(sequencer);
            }
        }

        info!("Running sequencers.");
        let sequencer_run_handles = sequencers
            .iter()
            .map(|sequencer| spawn_run_node(sequencer.node_config_path.clone()))
            .collect::<Vec<_>>();

        Self { sequencers, sequencer_run_handles }
    }

    pub async fn await_alive(&self, interval: u64, max_attempts: usize) {
        for (sequencer_index, sequencer) in self.sequencers.iter().enumerate() {
            sequencer
                .is_alive_test_client
                .await_alive(interval, max_attempts)
                .await
                .unwrap_or_else(|_| panic!("Node {} should be alive.", sequencer_index));
        }
    }

    pub async fn send_rpc_tx_fn(&self, rpc_tx: RpcTransaction) -> TransactionHash {
        self.sequencers[0].assert_add_tx_success(rpc_tx).await
    }

    pub fn batcher_storage_reader(&self) -> StorageReader {
        let (batcher_storage_reader, _) =
            papyrus_storage::open_storage(self.sequencers[0].batcher_storage_config.clone())
                .expect("Failed to open batcher's storage");
        batcher_storage_reader
    }

    pub fn shutdown_nodes(&self) {
        self.sequencer_run_handles.iter().for_each(|handle| {
            assert!(!handle.is_finished(), "Node should still be running.");
            handle.abort()
        });
    }
}

pub struct IntegrationSequencerSetup {
    /// Used to differentiate between different sequencer nodes.
    pub sequencer_index: usize,

    // Client for adding transactions to the sequencer node.
    pub add_tx_http_client: HttpTestClient,

    // Client for checking liveness of the sequencer node.
    pub is_alive_test_client: IsAliveClient,
    // Path to the node configuration file.
    pub node_config_path: PathBuf,
    // Storage reader for the batcher.
    pub batcher_storage_config: StorageConfig,
    // Storage reader for the state sync.
    pub state_sync_storage_config: StorageConfig,
    // Handlers for the storage and config files, maintained so the files are not deleted. Since
    // these are only maintained to avoid dropping the handlers, private visibility suffices, and
    // as such, the '#[allow(dead_code)]' attributes are used to suppress the warning.
    #[allow(dead_code)]
    batcher_storage_handle: TempDir,
    #[allow(dead_code)]
    node_config_dir_handle: TempDir,
    #[allow(dead_code)]
    state_sync_storage_handle: TempDir,
}

impl IntegrationSequencerSetup {
    #[instrument(skip(accounts, chain_info, consensus_manager_config), level = "debug")]
    pub async fn new(
        accounts: Vec<Contract>,
        sequencer_index: usize,
        chain_info: ChainInfo,
        mut consensus_manager_config: ConsensusManagerConfig,
        mempool_p2p_config: MempoolP2pConfig,
        available_ports: &mut AvailablePorts,
        component_config: ComponentConfig,
    ) -> Self {
        // Creating the storage for the test.
        let storage_for_test = StorageTestSetup::new(accounts, &chain_info);

<<<<<<< HEAD
=======
        // Spawn a papyrus rpc server for a papyrus storage reader.
        let rpc_server_addr = spawn_test_rpc_state_reader_with_socket(
            storage_for_test.rpc_storage_reader,
            chain_info.chain_id.clone(),
            available_ports.get_next_local_host_socket(),
        )
        .await;

        let recorder_url = spawn_success_recorder(available_ports.get_next_port());
        consensus_manager_config.cende_config.recorder_url = recorder_url;

>>>>>>> fdde48ea5 (refactor(sequencing): cende context, add logic and tests)
        // Derive the configuration for the sequencer node.
        let (config, required_params) = create_node_config(
            available_ports,
            sequencer_index,
            chain_info,
            storage_for_test.batcher_storage_config,
            storage_for_test.state_sync_storage_config,
            consensus_manager_config,
            mempool_p2p_config,
            component_config,
        )
        .await;

        let node_config_dir_handle = tempdir().unwrap();
        let node_config_path = dump_config_file_changes(
            &config,
            required_params,
            node_config_dir_handle.path().to_path_buf(),
        );

        // Wait for the node to start.
        let MonitoringEndpointConfig { ip, port, .. } = config.monitoring_endpoint_config;
        let is_alive_test_client = IsAliveClient::new(SocketAddr::from((ip, port)));

        let HttpServerConfig { ip, port } = config.http_server_config;
        let add_tx_http_client = HttpTestClient::new(SocketAddr::from((ip, port)));

        Self {
            sequencer_index,
            add_tx_http_client,
            is_alive_test_client,
            batcher_storage_handle: storage_for_test.batcher_storage_handle,
            batcher_storage_config: config.batcher_config.storage,
            node_config_dir_handle,
            node_config_path,
            state_sync_storage_handle: storage_for_test.state_sync_storage_handle,
            state_sync_storage_config: config.state_sync_config.storage_config,
        }
    }

    pub async fn assert_add_tx_success(&self, tx: RpcTransaction) -> TransactionHash {
        self.add_tx_http_client.assert_add_tx_success(tx).await
    }
}
