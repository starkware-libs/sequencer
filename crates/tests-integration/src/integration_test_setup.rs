use std::net::SocketAddr;

use blockifier::test_utils::contracts::FeatureContract;
use blockifier::test_utils::CairoVersion;
use starknet_api::executable_transaction::Transaction;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_gateway::config::GatewayNetworkConfig;
use starknet_gateway::errors::GatewaySpecError;
use starknet_mempool_infra::trace_util::configure_tracing;
use starknet_mempool_node::servers::get_server_future;
use starknet_mempool_node::utils::create_clients_servers_from_config;
use starknet_task_executor::tokio_executor::TokioExecutor;
use tokio::runtime::Handle;
use tokio::task::JoinHandle;

use crate::integration_test_utils::{create_config, HttpTestClient};
use crate::mock_batcher::MockBatcher;
use crate::state_reader::spawn_test_rpc_state_reader;

pub struct IntegrationTestSetup {
    pub task_executor: TokioExecutor,
    pub http_test_client: HttpTestClient,
    pub batcher: MockBatcher,
    pub gateway_handle: JoinHandle<()>,
    pub mempool_handle: JoinHandle<()>,
}

impl IntegrationTestSetup {
    pub async fn new(n_accounts: usize) -> Self {
        let default_account_contract =
            FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1);
        let accounts = std::iter::repeat(default_account_contract).take(n_accounts);
        Self::new_for_account_contracts(accounts).await
    }

    pub async fn new_for_account_contracts(
        accounts: impl IntoIterator<Item = FeatureContract>,
    ) -> Self {
        let handle = Handle::current();
        let task_executor = TokioExecutor::new(handle);

        // Configure and start tracing
        configure_tracing();

        // Spawn a papyrus rpc server for a papyrus storage reader.
        let rpc_server_addr = spawn_test_rpc_state_reader(accounts).await;

        // Derive the configuration for the mempool node.
        let config = create_config(rpc_server_addr).await;

        let (clients, servers) = create_clients_servers_from_config(&config);

        let GatewayNetworkConfig { ip, port } = config.gateway_config.network_config;
        let http_test_client = HttpTestClient::new(SocketAddr::from((ip, port)));

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

        Self { task_executor, http_test_client, batcher, gateway_handle, mempool_handle }
    }

    pub async fn assert_add_tx_success(&self, tx: &RpcTransaction) -> TransactionHash {
        self.http_test_client.assert_add_tx_success(tx).await
    }

    pub async fn assert_add_tx_error(&self, tx: &RpcTransaction) -> GatewaySpecError {
        self.http_test_client.assert_add_tx_error(tx).await
    }

    pub async fn get_txs(&self, n_txs: usize) -> Vec<Transaction> {
        self.batcher.get_txs(n_txs).await
    }
}
