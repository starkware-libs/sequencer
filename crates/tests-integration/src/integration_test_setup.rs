use std::net::SocketAddr;

use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use starknet_api::executable_transaction::Transaction;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_batcher_types::communication::SharedBatcherClient;
use starknet_gateway_types::errors::GatewaySpecError;
use starknet_http_server::config::HttpServerConfig;
use starknet_mempool_node::servers::run_component_servers;
use starknet_mempool_node::utils::create_node_modules;
use starknet_mempool_types::communication::SharedMempoolClient;
use starknet_sequencer_infra::trace_util::configure_tracing;
use starknet_task_executor::tokio_executor::TokioExecutor;
use tempfile::TempDir;
use tokio::runtime::Handle;
use tokio::task::JoinHandle;

use crate::integration_test_utils::{create_config, HttpTestClient};
use crate::state_reader::{spawn_test_rpc_state_reader, StorageTestSetup};

pub struct IntegrationTestSetup {
    pub task_executor: TokioExecutor,

    // Client for adding transactions to the sequencer node.
    pub add_tx_http_client: HttpTestClient,

    // Handlers for the storage files, maintained so the files are not deleted.
    pub batcher_storage_file_handle: TempDir,
    pub rpc_storage_file_handle: TempDir,

    // TODO(Arni): Replace with a batcher server handle and a batcher client.
    pub mempool_client: SharedMempoolClient,
    pub batcher_client: SharedBatcherClient,

    // Handle of the sequencer node.
    pub sequencer_node_handle: JoinHandle<Result<(), anyhow::Error>>,
}

impl IntegrationTestSetup {
    pub async fn new_from_tx_generator(tx_generator: &MultiAccountTransactionGenerator) -> Self {
        let handle = Handle::current();
        let task_executor = TokioExecutor::new(handle);

        // Configure and start tracing.
        configure_tracing();

        let accounts = tx_generator.accounts();
        let storage_for_test = StorageTestSetup::new(accounts);

        // Spawn a papyrus rpc server for a papyrus storage reader.
        let rpc_server_addr = spawn_test_rpc_state_reader(
            storage_for_test.rpc_storage_reader,
            storage_for_test.chain_id,
        )
        .await;

        // Derive the configuration for the mempool node.
        let config = create_config(rpc_server_addr, storage_for_test.batcher_storage_config).await;

        let (clients, servers) = create_node_modules(&config);

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
            mempool_client: clients.get_mempool_client().unwrap().clone(),
            batcher_client: clients.get_batcher_client().unwrap(),
            rpc_storage_file_handle: storage_for_test.rpc_storage_handle,
            sequencer_node_handle,
        }
    }

    pub async fn assert_add_tx_success(&self, tx: RpcTransaction) -> TransactionHash {
        self.add_tx_http_client.assert_add_tx_success(tx).await
    }

    pub async fn assert_add_tx_error(&self, tx: RpcTransaction) -> GatewaySpecError {
        self.add_tx_http_client.assert_add_tx_error(tx).await
    }

    // TODO(Arni): consider deleting this function if it is not used in any test.
    pub async fn get_txs(&self, n_txs: usize) -> Vec<Transaction> {
        self.mempool_client.get_txs(n_txs).await.unwrap()
    }
}
