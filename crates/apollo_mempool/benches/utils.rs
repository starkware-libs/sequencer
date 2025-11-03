use std::collections::HashMap;
use std::sync::Arc;

use apollo_config_manager_types::communication::MockConfigManagerClient;
use apollo_infra::component_server::{ComponentServerStarter, LocalServerConfig};
use apollo_infra::metrics::{LocalClientMetrics, LocalServerMetrics};
use apollo_mempool::communication::{create_mempool, LocalMempoolServer};
use apollo_mempool_config::config::MempoolConfig;
use apollo_mempool_p2p_types::communication::{
    MempoolP2pPropagatorClient,
    MempoolP2pPropagatorClientResult,
};
use apollo_mempool_types::communication::{
    AddTransactionArgsWrapper,
    LocalMempoolClient,
    SharedMempoolClient,
};
use apollo_mempool_types::mempool_types::{AccountState, AddTransactionArgs, CommitBlockArgs};
use apollo_metrics::metrics::{LabeledMetricHistogram, MetricCounter, MetricGauge, MetricScope};
use apollo_network_types::network_types::BroadcastedMessageMetadata;
use async_trait::async_trait;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::FeatureContract;
use indexmap::IndexSet;
use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::rpc_transaction::{
    InternalRpcTransaction,
    InternalRpcTransactionWithoutTxHash,
    RpcTransaction,
};
use starknet_api::{nonce, tx_hash};
use tokio::sync::mpsc;

/// Minimal overhead P2P propagator for benchmarking
/// All methods return Ok(()) immediately without any processing
pub struct BenchMempoolP2pPropagator;

#[async_trait]
impl MempoolP2pPropagatorClient for BenchMempoolP2pPropagator {
    async fn add_transaction(
        &self,
        _transaction: InternalRpcTransaction,
    ) -> MempoolP2pPropagatorClientResult<()> {
        Ok(())
    }

    async fn continue_propagation(
        &self,
        _propagation_metadata: BroadcastedMessageMetadata,
    ) -> MempoolP2pPropagatorClientResult<()> {
        Ok(())
    }

    async fn broadcast_queued_transactions(&self) -> MempoolP2pPropagatorClientResult<()> {
        Ok(())
    }
}

// Dummy metrics for benchmarking (we don't need real metrics collection)
const BENCH_MSGS_RECEIVED: MetricCounter = MetricCounter::new(
    MetricScope::Infra,
    "bench_msgs_received",
    "Benchmark messages received",
    0, // initial_value
);

const BENCH_MSGS_PROCESSED: MetricCounter = MetricCounter::new(
    MetricScope::Infra,
    "bench_msgs_processed",
    "Benchmark messages processed",
    0, // initial_value
);

const BENCH_HIGH_PRIORITY_QUEUE_DEPTH: MetricGauge = MetricGauge::new(
    MetricScope::Infra,
    "bench_high_priority_queue_depth",
    "Benchmark high priority queue depth",
);

const BENCH_NORMAL_PRIORITY_QUEUE_DEPTH: MetricGauge = MetricGauge::new(
    MetricScope::Infra,
    "bench_normal_priority_queue_depth",
    "Benchmark normal priority queue depth",
);

// Label permutations for histograms
const BENCH_LABEL_PERMUTATIONS: &[&[(&str, &str)]] =
    &[&[("request_type", "add_tx")], &[("request_type", "get_txs")]];

const BENCH_PROCESSING_TIMES: LabeledMetricHistogram = LabeledMetricHistogram::new(
    MetricScope::Infra,
    "bench_processing_times",
    "Benchmark processing times",
    BENCH_LABEL_PERMUTATIONS,
);

const BENCH_QUEUEING_TIMES: LabeledMetricHistogram = LabeledMetricHistogram::new(
    MetricScope::Infra,
    "bench_queueing_times",
    "Benchmark queueing times",
    BENCH_LABEL_PERMUTATIONS,
);

const BENCH_RESPONSE_TIMES: LabeledMetricHistogram = LabeledMetricHistogram::new(
    MetricScope::Infra,
    "bench_response_times",
    "Benchmark response times",
    BENCH_LABEL_PERMUTATIONS,
);

// Static metrics instances for benchmark
const BENCH_LOCAL_SERVER_METRICS: LocalServerMetrics = LocalServerMetrics::new(
    &BENCH_MSGS_RECEIVED,
    &BENCH_MSGS_PROCESSED,
    &BENCH_HIGH_PRIORITY_QUEUE_DEPTH,
    &BENCH_NORMAL_PRIORITY_QUEUE_DEPTH,
    &BENCH_PROCESSING_TIMES,
    &BENCH_QUEUEING_TIMES,
);

const BENCH_LOCAL_CLIENT_METRICS: LocalClientMetrics =
    LocalClientMetrics::new(&BENCH_RESPONSE_TIMES);

struct TransactionGenerator {
    multi_tx_generator: MultiAccountTransactionGenerator,
    sender_address: ContractAddress,
}

impl TransactionGenerator {
    fn new(cairo_version: CairoVersion) -> Self {
        let mut multi_tx_generator = MultiAccountTransactionGenerator::new();
        let account_type = FeatureContract::AccountWithoutValidations(cairo_version);
        multi_tx_generator.register_deployed_account(account_type);
        let sender_address = multi_tx_generator.account_with_id(0).sender_address();
        Self { multi_tx_generator, sender_address }
    }

    fn generate_invoke(&mut self, index: usize) -> AddTransactionArgs {
        let RpcTransaction::Invoke(invoke_tx) =
            self.multi_tx_generator.account_with_id_mut(0).generate_trivial_rpc_invoke_tx(0)
        else {
            panic!("Expected RpcTransaction::Invoke")
        };

        AddTransactionArgs {
            tx: InternalRpcTransaction {
                tx: InternalRpcTransactionWithoutTxHash::Invoke(invoke_tx),
                tx_hash: tx_hash!(index + 100), // Use index to create a unique hash
            },
            // Since generate_invoke_with_tip() creates first transaction with nonce 1 (first
            // invoke after deploy), the AccountState must also have nonce 1 to initialize the
            // mempool nonce management to 1.
            account_state: AccountState { address: self.sender_address, nonce: nonce!(1) },
        }
    }
}

#[derive(Clone)]
pub struct BenchTestSetupConfig {
    pub n_txs: usize,
    pub mempool_config: MempoolConfig,
    pub chunk_size: usize, // Number of "add_tx" requests per one "get_tx" request.
}

pub struct BenchTestSetup {
    config: BenchTestSetupConfig,
    txs: Vec<AddTransactionArgs>,
}

/// Server-Client setup for realistic benchmarking
pub struct MempoolServerClientSetup {
    pub client: SharedMempoolClient,
    _server_handle: tokio::task::JoinHandle<()>,
}

impl BenchTestSetup {
    pub fn new(config: &BenchTestSetupConfig) -> Self {
        let cairo_version = CairoVersion::Cairo1(RunnableCairo1::Casm);
        let mut tx_generator = TransactionGenerator::new(cairo_version);

        let txs = (0..config.n_txs).map(|i| tx_generator.generate_invoke(i)).collect();

        Self { config: config.clone(), txs }
    }

    /// Creates a server-client setup for realistic benchmarking
    /// This simulates how mempool is accessed in the real application
    pub async fn create_server_client_setup(&self) -> MempoolServerClientSetup {
        // Create server configuration
        let server_config = LocalServerConfig::default();

        // Create communication channels
        let (tx, rx) = mpsc::channel(server_config.inbound_requests_channel_capacity);

        // Create minimal overhead P2P client for benchmark
        let bench_p2p_client = Arc::new(BenchMempoolP2pPropagator);

        // Create mock config manager client for benchmark
        let mut mock_config_manager = MockConfigManagerClient::new();
        let dynamic_config = self.config.mempool_config.dynamic_config.clone();
        mock_config_manager
            .expect_get_mempool_dynamic_config()
            .returning(move || Ok(dynamic_config.clone()));
        let mock_config_manager = Arc::new(mock_config_manager);

        // Create the mempool component
        let mempool_component = create_mempool(
            self.config.mempool_config.clone(),
            bench_p2p_client,
            mock_config_manager,
        );

        // Use static metrics for benchmark
        let server_metrics = &BENCH_LOCAL_SERVER_METRICS;
        let client_metrics = &BENCH_LOCAL_CLIENT_METRICS;

        // Create the server
        let server = LocalMempoolServer::new(mempool_component, &server_config, rx, server_metrics);

        // Start the server in a background task
        // Note: LocalComponentServer::start() will panic when it finishes processing
        // all requests. This is the expected behavior and not an error.
        let server_handle = tokio::spawn(async move {
            let mut server = server;
            let _ = server.start().await; // Expected panic when server finishes
        });

        // Create the client
        let client = LocalMempoolClient::new(tx, client_metrics);
        let shared_client: SharedMempoolClient = Arc::new(client);

        // Give the server a moment to fully start
        tokio::task::yield_now().await;

        MempoolServerClientSetup { client: shared_client, _server_handle: server_handle }
    }

    /// Task that continuously adds transactions to the mempool via client.
    /// Simulates concurrent producers in a real system
    async fn add_tx_task(client: SharedMempoolClient, txs: Vec<AddTransactionArgs>) {
        for tx in txs.into_iter() {
            let wrapped_args = AddTransactionArgsWrapper { args: tx, p2p_message_metadata: None };

            client
                .add_tx(wrapped_args)
                .await
                .unwrap_or_else(|e| panic!("Failed to add tx to mempool: {e:?}"));
        }
    }

    /// Task that continuously retrieves transactions from the mempool via client.
    /// Simulates concurrent consumers in a real system
    async fn get_txs_task(client: SharedMempoolClient, n_txs: usize, chunk_size: usize) {
        let mut txs_received = 0;
        let mut last_committed_chunk = 0;

        while txs_received < n_txs {
            let retrieved_txs = client
                .get_txs(chunk_size)
                .await
                .unwrap_or_else(|e| panic!("Failed to get txs from mempool: {e:?}"));

            txs_received += retrieved_txs.len();

            // Commit a block when we've accumulated enough transactions for a complete chunk.
            // This simulates the block production cycle where transactions are periodically
            // committed after being retrieved from the mempool.
            if last_committed_chunk < txs_received / chunk_size {
                last_committed_chunk += 1;

                // Aggregate the highest nonce for each contract address in this chunk.
                let address_to_nonce = retrieved_txs
                    .iter()
                    .map(|tx| (tx.contract_address(), tx.nonce()))
                    .fold(HashMap::<ContractAddress, Nonce>::new(), |mut acc, (address, nonce)| {
                        acc.entry(address)
                            .and_modify(|max_nonce| *max_nonce = (*max_nonce).max(nonce))
                            .or_insert(nonce);
                        acc
                    });

                client
                    .commit_block(CommitBlockArgs {
                        address_to_nonce,
                        rejected_tx_hashes: IndexSet::new(),
                    })
                    .await
                    .unwrap();
            }

            // If no txs retrieved, wait a bit for add_tx_task to add more
            if retrieved_txs.is_empty() {
                tokio::time::sleep(tokio::time::Duration::from_micros(100)).await;
            }
        }
    }

    /// Concurrent benchmark using server-client architecture
    /// This simulates realistic concurrent access patterns to the mempool
    pub async fn mempool_add_get_txs(&self) {
        // Create server-client setup for realistic benchmarking
        let server_client_setup = self.create_server_client_setup().await;
        let client = Arc::clone(&server_client_setup.client);

        // Create tasks for concurrent execution
        let add_task = tokio::spawn(Self::add_tx_task(Arc::clone(&client), self.txs.clone()));
        let get_task = tokio::spawn(Self::get_txs_task(
            Arc::clone(&client),
            self.config.n_txs,
            self.config.chunk_size,
        ));

        // Wait for both tasks to complete
        // Using try_join! for better error propagation in benchmarks
        tokio::try_join!(add_task, get_task)
            .expect("One or both tasks failed during benchmark execution");
    }
}
