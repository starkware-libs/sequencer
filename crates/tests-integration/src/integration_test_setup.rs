use std::net::SocketAddr;

use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use papyrus_storage::test_utils::{get_test_storage, get_test_storage_with_config_by_scope};
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
use crate::state_reader::{create_test_state, spawn_test_rpc_state_reader};

pub struct IntegrationTestSetup {
    pub task_executor: TokioExecutor,
    pub http_test_client: HttpTestClient,

    pub batcher_storage_file_handle: TempDir,
    // TODO(Arni): Replace with a batcher server handle and a batcher client.
    pub mempool_client: SharedMempoolClient,

    pub rpc_storage_file_handle: TempDir,
    pub gateway_handle: JoinHandle<Result<(), ComponentServerError>>,

    pub http_server_handle: JoinHandle<Result<(), ComponentServerError>>,
    pub mempool_handle: JoinHandle<Result<(), ComponentServerError>>,
}

impl IntegrationTestSetup {
    pub async fn new_from_tx_generator(tx_generator: &MultiAccountTransactionGenerator) -> Self {
        let handle = Handle::current();
        let task_executor = TokioExecutor::new(handle);

        // Configure and start tracing.
        configure_tracing();

        let accounts = tx_generator.accounts();

        let ((rpc_storage_reader, mut rpc_storage_writer), rpc_storage_file_handle) =
            get_test_storage();
        create_test_state(&mut rpc_storage_writer, accounts.clone());

        // Spawn a papyrus rpc server for a papyrus storage reader.
        let rpc_server_addr = spawn_test_rpc_state_reader(rpc_storage_reader).await;

        // Create a storage with the initial state for the batcher and drop the storage handles (the
        // batchers opens the storage independently).
        let (batcher_storage_config, batcher_storage_file_handle) = {
            let (
                (_, mut batcher_storage_writer),
                batcher_storage_config,
                batcher_storage_file_handle,
            ) = get_test_storage_with_config_by_scope(papyrus_storage::StorageScope::StateOnly);
            create_test_state(&mut batcher_storage_writer, accounts.clone());
            (batcher_storage_config, batcher_storage_file_handle)
        };

        // Derive the configuration for the mempool node.
        let config = create_config(rpc_server_addr, batcher_storage_config).await;

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

        // Build and run mempool.
        let mempool_future = get_server_future("Mempool", true, servers.local_servers.mempool);
        let mempool_handle = task_executor.spawn_with_handle(mempool_future);

        Self {
            task_executor,
            http_test_client,
            batcher_storage_file_handle,
            mempool_client: clients.get_mempool_client().unwrap().clone(),
            rpc_storage_file_handle,
            gateway_handle,
            http_server_handle,
            mempool_handle,
        }
    }

    pub async fn assert_add_tx_success(&self, tx: &RpcTransaction) -> TransactionHash {
        self.http_test_client.assert_add_tx_success(tx).await
    }

    pub async fn assert_add_tx_error(&self, tx: &RpcTransaction) -> GatewaySpecError {
        self.http_test_client.assert_add_tx_error(tx).await
    }

    pub async fn get_txs(&self, n_txs: usize) -> Vec<Transaction> {
        self.mempool_client.get_txs(n_txs).await.unwrap()
    }
}
