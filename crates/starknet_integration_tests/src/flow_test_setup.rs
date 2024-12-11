use std::net::SocketAddr;

use blockifier::context::ChainInfo;
use mempool_test_utils::starknet_api_test_utils::{Contract, MultiAccountTransactionGenerator};
use papyrus_network::network_manager::BroadcastTopicChannels;
use papyrus_protobuf::consensus::{ProposalPart, StreamMessage};
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_consensus_manager::config::ConsensusManagerConfig;
use starknet_gateway_types::errors::GatewaySpecError;
use starknet_http_server::config::HttpServerConfig;
use starknet_http_server::test_utils::HttpTestClient;
use starknet_sequencer_node::config::node_config::SequencerNodeConfig;
use starknet_sequencer_node::servers::run_component_servers;
use starknet_sequencer_node::utils::create_node_modules;
use starknet_task_executor::tokio_executor::TokioExecutor;
use tempfile::TempDir;
use tokio::runtime::Handle;
use tokio::task::JoinHandle;
use tracing::{debug, instrument};

use crate::definitions::MockSystemMode;
use crate::state_reader::{spawn_test_rpc_state_reader, StorageTestSetup};
use crate::utils::{
    create_chain_info,
    create_config,
    create_consensus_manager_configs_and_channels,
    get_http_server_config,
};

const SEQUENCER_0: usize = 0;
const SEQUENCER_1: usize = 1;
const SEQUENCER_INDICES: [usize; 2] = [SEQUENCER_0, SEQUENCER_1];

pub struct FlowTestSetup {
    // TODO(Tsabary): Remove this field.
    pub task_executor: TokioExecutor,
    pub sequencer_0: SequencerSetup,
    pub sequencer_1: SequencerSetup,

    // Channels for consensus proposals, used for asserting the right transactions are proposed.
    pub consensus_proposals_channels: BroadcastTopicChannels<StreamMessage<ProposalPart>>,
}

impl FlowTestSetup {
    #[instrument(skip(tx_generator), level = "debug")]
    pub async fn new_from_tx_generator(
        tx_generator: &MultiAccountTransactionGenerator,
        mock_system_mode: MockSystemMode,
    ) -> Self {
        let handle = Handle::current();
        let task_executor = TokioExecutor::new(handle);
        let chain_info = create_chain_info();

        let accounts = tx_generator.accounts();
        let (mut consensus_manager_configs, consensus_proposals_channels) =
            create_consensus_manager_configs_and_channels(SEQUENCER_INDICES.len());

        // Take the first config for every sequencer node, and create nodes one after the other in
        // order to make sure the ports are not overlapping.
        let sequencer_0_consensus_manager_config = consensus_manager_configs.remove(0);
        let sequencer_0 = SequencerSetup::new(
            accounts.clone(),
            SEQUENCER_0,
            chain_info.clone(),
            &task_executor,
            sequencer_0_consensus_manager_config,
            mock_system_mode.clone(),
        )
        .await;

        let sequencer_1_consensus_manager_config = consensus_manager_configs.remove(0);
        let sequencer_1 = SequencerSetup::new(
            accounts,
            SEQUENCER_1,
            chain_info,
            &task_executor,
            sequencer_1_consensus_manager_config,
            mock_system_mode,
        )
        .await;

        Self { task_executor, sequencer_0, sequencer_1, consensus_proposals_channels }
    }

    pub async fn assert_add_tx_error(&self, tx: RpcTransaction) -> GatewaySpecError {
        self.sequencer_0.add_tx_http_client.assert_add_tx_error(tx).await
    }
}

pub struct SequencerSetup {
    /// Used to differentiate between different sequencer nodes.
    pub sequencer_index: usize,

    // Client for adding transactions to the sequencer node.
    pub add_tx_http_client: HttpTestClient,

    // Handlers for the storage files, maintained so the files are not deleted.
    pub batcher_storage_file_handle: TempDir,
    pub rpc_storage_file_handle: TempDir,

    // Handle of the sequencer node.
    pub sequencer_node_handles: Vec<JoinHandle<Result<(), anyhow::Error>>>,

    pub configs: Vec<SequencerNodeConfig>,
}

impl SequencerSetup {
    #[instrument(
        skip(accounts, chain_info, task_executor, consensus_manager_config),
        level = "debug"
    )]
    pub async fn new(
        accounts: Vec<Contract>,
        sequencer_index: usize,
        chain_info: ChainInfo,
        task_executor: &TokioExecutor,
        consensus_manager_config: ConsensusManagerConfig,
        mock_system_mode: MockSystemMode,
    ) -> Self {
        let storage_for_test = StorageTestSetup::new(accounts, &chain_info);

        // Spawn a papyrus rpc server for a papyrus storage reader.
        let rpc_server_addr = spawn_test_rpc_state_reader(
            storage_for_test.rpc_storage_reader,
            chain_info.chain_id.clone(),
        )
        .await;

        // Derive the configuration for the sequencer node.
        let (configs, _required_params) = create_config(
            sequencer_index,
            chain_info,
            rpc_server_addr,
            storage_for_test.batcher_storage_config,
            consensus_manager_config,
            mock_system_mode,
        )
        .await;

        debug!("Sequencer configs: {:#?}", configs);

        let mut sequencer_clients = vec![];
        let mut sequencer_servers = vec![];
        for config in configs.iter() {
            let (clients, servers) = create_node_modules(config);
            sequencer_clients.push(clients);
            sequencer_servers.push(servers);
        }

        let HttpServerConfig { ip, port } =
            get_http_server_config(&configs).expect("No matching HttpServerConfig found");
        let add_tx_http_client = HttpTestClient::new(SocketAddr::from((*ip, *port)));

        // Build and run the sequencer node.
        let mut sequencer_node_handles = vec![];
        for server in sequencer_servers {
            let sequencer_node_future = run_component_servers(server);
            let sequencer_node_handle = task_executor.spawn_with_handle(sequencer_node_future);
            sequencer_node_handles.push(sequencer_node_handle);
        }

        // Wait for server to spin up.
        // TODO(Gilad): Replace with a persistent Client with a built-in retry to protect against CI
        // flakiness.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        Self {
            sequencer_index,
            add_tx_http_client,
            batcher_storage_file_handle: storage_for_test.batcher_storage_handle,
            rpc_storage_file_handle: storage_for_test.rpc_storage_handle,
            sequencer_node_handles,
            configs,
        }
    }

    pub async fn assert_add_tx_success(&self, tx: RpcTransaction) -> TransactionHash {
        self.add_tx_http_client.assert_add_tx_success(tx).await
    }
}
