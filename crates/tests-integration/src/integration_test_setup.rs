use std::net::SocketAddr;

use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use starknet_api::executable_transaction::Transaction;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_gateway_types::errors::GatewaySpecError;
use starknet_http_server::config::HttpServerConfig;
use starknet_mempool_infra::errors::ComponentServerError;
use starknet_mempool_infra::trace_util::configure_tracing;
use starknet_mempool_node::servers::get_server_future;
use starknet_mempool_node::utils::create_node_modules;
use starknet_mempool_types::communication::SharedMempoolClient;
use starknet_task_executor::tokio_executor::TokioExecutor;
use tempfile::TempDir;
use tokio::runtime::Handle;
use tokio::task::JoinHandle;

use crate::integration_test_utils::{create_config, HttpTestClient};
use crate::mock_consensus_manager::MockConsensusManager;
use crate::state_reader::{spawn_test_rpc_state_reader, StorageTestSetup};

pub struct IntegrationTestSetup {
    pub task_executor: TokioExecutor,
    pub http_test_client: HttpTestClient,

    pub batcher_storage_file_handle: TempDir,
    // TODO(Arni): Replace with a batcher server handle and a batcher client.
    pub mempool_client: SharedMempoolClient,
    pub mock_consensus_manager: MockConsensusManager,

    pub rpc_storage_file_handle: TempDir,
    pub gateway_handle: JoinHandle<Result<(), ComponentServerError>>,

    pub http_server_handle: JoinHandle<Result<(), ComponentServerError>>,
    pub mempool_handle: JoinHandle<Result<(), ComponentServerError>>,
    pub batcher_handle: JoinHandle<Result<(), ComponentServerError>>,
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
        let rpc_server_addr =
            spawn_test_rpc_state_reader(storage_for_test.rpc_storage_reader).await;

        // Derive the configuration for the mempool node.
        let config = create_config(rpc_server_addr, storage_for_test.batcher_storage_config).await;

        let (clients, servers) = create_node_modules(&config);

        let HttpServerConfig { ip, port } = config.http_server_config;
        let http_test_client = HttpTestClient::new(SocketAddr::from((ip, port)));

        let gateway_future = get_server_future("Gateway", true, servers.local_servers.gateway);
        let gateway_handle = task_executor.spawn_with_handle(gateway_future);

        let http_server_future =
            get_server_future("HttpServer", true, servers.wrapper_servers.http_server);
        let http_server_handle = task_executor.spawn_with_handle(http_server_future);

        // Wait for server to spin up.
        // TODO(Gilad): Replace with a persistant Client with a built-in retry mechanism,
        // to avoid the sleep and to protect against CI flakiness.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Build and run Batcher.
        let batcher_future = get_server_future("Batcher", true, servers.local_servers.batcher);
        let batcher_handle = task_executor.spawn_with_handle(batcher_future);

        // Build and run mempool.
        let mempool_future = get_server_future("Mempool", true, servers.local_servers.mempool);
        let mempool_handle = task_executor.spawn_with_handle(mempool_future);

        Self {
            task_executor,
            http_test_client,
            batcher_storage_file_handle: storage_for_test.batcher_storage_handle,
            mempool_client: clients.get_mempool_client().unwrap().clone(),
            rpc_storage_file_handle: storage_for_test.rpc_storage_handle,
            mock_consensus_manager: MockConsensusManager {
                batcher_client: clients.get_batcher_client().unwrap(),
            },
            gateway_handle,
            http_server_handle,
            mempool_handle,
            batcher_handle,
        }
    }

    pub async fn assert_add_tx_success(&self, tx: &RpcTransaction) -> TransactionHash {
        self.http_test_client.assert_add_tx_success(tx).await
    }

    pub async fn assert_add_tx_error(&self, tx: &RpcTransaction) -> GatewaySpecError {
        self.http_test_client.assert_add_tx_error(tx).await
    }

    // TODO(Arni): consider deleting this function if it is not used in any test.
    pub async fn get_txs(&self, n_txs: usize) -> Vec<Transaction> {
        self.mempool_client.get_txs(n_txs).await.unwrap()
    }
}
