use std::net::SocketAddr;

use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use papyrus_network::network_manager::BroadcastTopicChannels;
use papyrus_protobuf::consensus::ProposalPart;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_gateway_types::errors::GatewaySpecError;
use starknet_http_server::config::HttpServerConfig;
use starknet_http_server::test_utils::HttpTestClient;
use starknet_sequencer_infra::trace_util::configure_tracing;
use starknet_sequencer_node::servers::run_component_servers;
use starknet_sequencer_node::utils::create_node_modules;
use starknet_task_executor::tokio_executor::TokioExecutor;
use tempfile::TempDir;
use tokio::runtime::Handle;
use tokio::task::JoinHandle;

use crate::state_reader::{spawn_test_rpc_state_reader, StorageTestSetup};
use crate::utils::{create_chain_info, create_config};

pub struct FlowTestSetup {
    pub task_executor: TokioExecutor,

    // Client for adding transactions to the sequencer node.
    pub add_tx_http_client: HttpTestClient,

    // Handlers for the storage files, maintained so the files are not deleted.
    pub batcher_storage_file_handle: TempDir,
    pub rpc_storage_file_handle: TempDir,

    // Handle of the sequencer node.
    pub sequencer_node_handle: JoinHandle<Result<(), anyhow::Error>>,

    // Channels for consensus proposals, used for asserting the right transactions are proposed.
    pub consensus_proposals_channels: BroadcastTopicChannels<ProposalPart>,
}

impl FlowTestSetup {
    pub async fn new_from_tx_generator(tx_generator: &MultiAccountTransactionGenerator) -> Self {
        let handle = Handle::current();
        let task_executor = TokioExecutor::new(handle);
        let chain_info = create_chain_info();

        // Configure and start tracing.
        configure_tracing();

        let accounts = tx_generator.accounts();
        let storage_for_test = StorageTestSetup::new(accounts, chain_info.chain_id.clone());

        // Spawn a papyrus rpc server for a papyrus storage reader.
        let rpc_server_addr = spawn_test_rpc_state_reader(
            storage_for_test.rpc_storage_reader,
            chain_info.chain_id.clone(),
        )
        .await;

        // Derive the configuration for the sequencer node.
        let (config, _required_params, consensus_proposals_channels) =
            create_config(chain_info, rpc_server_addr, storage_for_test.batcher_storage_config)
                .await;

        let (_clients, servers) = create_node_modules(&config);

        let HttpServerConfig { ip, port } = config.http_server_config;
        let add_tx_http_client = HttpTestClient::new(SocketAddr::from((ip, port)));

        // Build and run the sequencer node.
        let sequencer_node_future = run_component_servers(servers);
        let sequencer_node_handle = task_executor.spawn_with_handle(sequencer_node_future);

        // Wait for server to spin up.
        // TODO(Gilad): Replace with a persistent Client with a built-in retry to protect against CI
        // flakiness.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        Self {
            task_executor,
            add_tx_http_client,
            batcher_storage_file_handle: storage_for_test.batcher_storage_handle,
            rpc_storage_file_handle: storage_for_test.rpc_storage_handle,
            sequencer_node_handle,
            consensus_proposals_channels,
        }
    }

    pub async fn assert_add_tx_success(&self, tx: RpcTransaction) -> TransactionHash {
        self.add_tx_http_client.assert_add_tx_success(tx).await
    }

    pub async fn assert_add_tx_error(&self, tx: RpcTransaction) -> GatewaySpecError {
        self.add_tx_http_client.assert_add_tx_error(tx).await
    }
}
