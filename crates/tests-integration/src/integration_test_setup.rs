use std::net::SocketAddr;

use starknet_api::rpc_transaction::RPCTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_gateway::config::GatewayNetworkConfig;
use starknet_gateway::errors::GatewayError;
use starknet_mempool_node::communication::{create_node_channels, create_node_clients};
use starknet_mempool_node::components::create_components;
use starknet_mempool_node::servers::{create_servers, get_server_future};
use starknet_mempool_types::mempool_types::ThinTransaction;
use starknet_task_executor::executor::TaskExecutor;
use starknet_task_executor::tokio_executor::TokioExecutor;
use tokio::runtime::Handle;
use tokio::task::JoinHandle;

use crate::integration_test_utils::{create_config, GatewayClient};
use crate::mock_batcher::MockBatcher;
use crate::state_reader::spawn_test_rpc_state_reader;

pub struct IntegrationTestSetup {
    pub task_executor: TokioExecutor,
    pub gateway_client: GatewayClient,
    pub batcher: MockBatcher,
    pub gateway_handle: JoinHandle<()>,
    pub mempool_handle: JoinHandle<()>,
}

impl IntegrationTestSetup {
    pub async fn new(n_accounts: usize) -> Self {
        let handle = Handle::current();
        let task_executor = TokioExecutor::new(handle);

        // Spawn a papyrus rpc server for a papyrus storage reader.
        let rpc_server_addr = spawn_test_rpc_state_reader(n_accounts).await;

        // Derive the configuration for the mempool node.
        let config = create_config(rpc_server_addr).await;

        // Create the communication network for the mempool node.
        let channels = create_node_channels();

        // Create the clients for the mempool node.
        let clients = create_node_clients(&config, &channels);

        // Create the components for the mempool node.
        let components = create_components(&config, &clients);

        // Create the servers for the mempool node.
        let servers = create_servers(&config, channels, components);

        let GatewayNetworkConfig { ip, port } = config.gateway_config.network_config;
        let gateway_client = GatewayClient::new(SocketAddr::from((ip, port)));

        let gateway_future = get_server_future("Gateway", true, servers.gateway);
        let gateway_handle = task_executor.spawn_with_handle(gateway_future);

        // Wait for server to spin up.
        // TODO(Gilad): Replace with a persistant Client with a built-in retry mechanism,
        // to avoid the sleep and to protect against CI flakiness.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Build Batcher.
        let batcher = MockBatcher::new(clients.get_mempool_client().unwrap());

        // Build and run mempool.
        let mempool_future = get_server_future("Mempool", true, servers.mempool);
        let mempool_handle = task_executor.spawn_with_handle(mempool_future);

        Self { task_executor, gateway_client, batcher, gateway_handle, mempool_handle }
    }

    pub async fn assert_add_tx_success(&self, tx: &RPCTransaction) -> TransactionHash {
        self.gateway_client.assert_add_tx_success(tx).await
    }

    pub async fn assert_add_tx_error(&self, tx: &RPCTransaction) -> GatewayError {
        self.gateway_client.assert_add_tx_error(tx).await
    }

    pub async fn get_txs(&self, n_txs: usize) -> Vec<ThinTransaction> {
        let batcher = self.batcher.clone();
        self.task_executor.spawn(async move { batcher.get_txs(n_txs).await }).await.unwrap()
    }
}
