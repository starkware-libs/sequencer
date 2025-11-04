//! Benchmark module for the blockifier crate. It provides functionalities to benchmark
//! various aspects related to transferring between accounts, including preparation
//! and execution of transfers.
//!
//! The main benchmark function is `transfers_benchmark`, which measures the performance
//! of transfers between randomly created accounts, which are iterated over round-robin.
//!
//! Run the benchmarks using `cargo bench --bench blockifier`.
//!
//! For Cairo Native compilation run the benchmarks using:
//! `cargo bench --bench blockifier --features "cairo_native"`.

use std::sync::Arc;

use apollo_infra_utils::set_global_allocator;
use blockifier::blockifier::concurrent_transaction_executor::ConcurrentTransactionExecutor;
use blockifier::blockifier::config::{ConcurrencyConfig, TransactionExecutorConfig};
use blockifier::blockifier::transaction_executor::TransactionExecutor;
use blockifier::concurrency::worker_pool::WorkerPool;
use blockifier::context::BlockContext;
use blockifier::state::cached_state::CachedState;
use blockifier::test_utils::dict_state_reader::DictStateReader;
use blockifier::test_utils::transfers_generator::{
    RecipientGeneratorType,
    TransfersGenerator,
    TransfersGeneratorConfig,
};
use blockifier::transaction::account_transaction::AccountTransaction;
use blockifier::transaction::test_utils::{create_test_init_data, TestInitData};
use blockifier::transaction::transaction_execution::Transaction;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::calldata::create_calldata;
use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use starknet_api::invoke_tx_args;
use starknet_api::test_utils::invoke::executable_invoke_tx;
use starknet_api::transaction::TransactionVersion;

/// The name of the benchmarks without the suffix.
const TRANSFERS_BENCHMARK_NAME: &str = "transfers_benchmark";
const HEAVY_TX_CONCURRENT_BENCHMARK_NAME: &str = "heavy_tx_benchmark_concurrent";
const HEAVY_TX_SEQUENTIAL_BENCHMARK_NAME: &str = "heavy_tx_benchmark_sequential";

/// Suffix for benchmark names to differentiate Cairo Native vs VM.
#[cfg(feature = "cairo_native")]
const BENCHMARK_NAME_SUFFIX: &str = "_cairo_native";
#[cfg(not(feature = "cairo_native"))]
const BENCHMARK_NAME_SUFFIX: &str = "_vm";

const HEAVY_TX_ENTRY_POINT: &str = "test_builtin_counts_consistency";

// TODO(Arni): Consider how to run this benchmark both with and without setting the allocator. Maybe
// hide this macro call under a feature, and run this benchmark regularly or with
// `cargo bench --bench blockifier --feature=specified_allocator`
set_global_allocator!();

/// Returns heavy transaction that calls HEAVY_TX_ENTRY_POINT,
/// and initializes the state with the test_init_data.
fn setup_heavy_tx_benchmark(
    block_context: &BlockContext,
) -> (Transaction, CachedState<DictStateReader>) {
    // Select Cairo version based on feature flag.
    #[cfg(feature = "cairo_native")]
    let cairo_version = CairoVersion::Cairo1(RunnableCairo1::Native);
    #[cfg(not(feature = "cairo_native"))]
    let cairo_version = CairoVersion::Cairo1(RunnableCairo1::Casm);

    let TestInitData { state, account_address, contract_address, mut nonce_manager } =
        create_test_init_data(block_context.chain_info(), cairo_version);

    let entry_point_args = vec![];
    let calldata = create_calldata(contract_address, HEAVY_TX_ENTRY_POINT, &entry_point_args);

    let invoke_tx = executable_invoke_tx(invoke_tx_args! {
        sender_address: account_address,
        calldata,
        nonce: nonce_manager.next(account_address),
        version: TransactionVersion::THREE,
    });
    let account_tx = AccountTransaction::new_for_sequencing(invoke_tx);
    let tx = Transaction::Account(account_tx);

    (tx, state)
}

/// Benchmarks the execution phase of the transfers flow.
/// The sender account is chosen round-robin.
/// The recipient account is chosen randomly.
/// The transactions are executed concurrently.
pub fn transfers_benchmark(c: &mut Criterion) {
    let transfers_generator_config = TransfersGeneratorConfig {
        recipient_generator_type: RecipientGeneratorType::Random,
        #[cfg(feature = "cairo_native")]
        cairo_version: CairoVersion::Cairo1(RunnableCairo1::Native),
        concurrency_config: ConcurrencyConfig::create_for_testing(false),
        ..Default::default()
    };
    let mut transfers_generator = TransfersGenerator::new(transfers_generator_config);
    // Benchmark only the execution phase (run_block_of_transfers call).
    // Transaction generation and state setup happen for each iteration but are not timed.
    c.bench_function(
        &format!("{}{}", TRANSFERS_BENCHMARK_NAME, BENCHMARK_NAME_SUFFIX),
        |benchmark| {
            benchmark.iter_batched(
                || {
                    // Setup: prepare transactions and executor (not measured).
                    transfers_generator.prepare_to_run_block_of_transfers(None)
                },
                |(txs, mut executor_wrapper)| {
                    // Measured: execute the transactions.
                    TransfersGenerator::run_block_of_transfers(&txs, &mut executor_wrapper, None)
                },
                BatchSize::SmallInput,
            )
        },
    );
}

/// Benchmarks the execution phase of `HEAVY_TX_ENTRY_POINT` using
/// ConcurrentTransactionExecutor.
pub fn heavy_tx_benchmark_concurrent(c: &mut Criterion) {
    let block_context = BlockContext::create_for_account_testing();

    let executor_config = TransactionExecutorConfig::create_for_testing(true);
    let worker_pool = Arc::new(WorkerPool::start(&executor_config.get_worker_pool_config()));

    let bench_name = format!("{}{}", HEAVY_TX_CONCURRENT_BENCHMARK_NAME, BENCHMARK_NAME_SUFFIX);
    c.bench_function(&bench_name, |benchmark| {
        benchmark.iter_batched(
            || {
                // Setup: prepare transaction and executor (not measured).
                let (tx, state) = setup_heavy_tx_benchmark(&block_context);

                let executor = ConcurrentTransactionExecutor::new_for_testing(
                    state,
                    block_context.clone(),
                    worker_pool.clone(),
                    None,
                );

                (tx, executor)
            },
            |(tx, mut executor)| {
                // Measured: execute the transaction.
                let results = executor.add_txs_and_wait(&[tx]);
                let tx_execution_info = &results[0].as_ref().unwrap().0;
                tx_execution_info.check_call_infos_native_execution(true);
                assert!(
                    !tx_execution_info.is_reverted(),
                    "Transaction reverted: {:?}",
                    tx_execution_info.revert_error
                );
                // Abort the block to allow the worker threads to continue to the next block.
                executor.abort_block();
            },
            BatchSize::SmallInput,
        )
    });

    // Cleanup worker pool after all benchmark iterations complete.
    Arc::try_unwrap(worker_pool).expect("More than one instance of worker pool exists").join();
}

/// Benchmarks the execution phase of `HEAVY_TX_ENTRY_POINT` using
/// TransactionExecutor (sequential).
pub fn heavy_tx_benchmark_sequential(c: &mut Criterion) {
    let block_context = BlockContext::create_for_account_testing();

    let bench_name = format!("{}{}", HEAVY_TX_SEQUENTIAL_BENCHMARK_NAME, BENCHMARK_NAME_SUFFIX);
    c.bench_function(&bench_name, |benchmark| {
        benchmark.iter_batched(
            || {
                // Setup: prepare transaction and executor (not measured).
                let (tx, state) = setup_heavy_tx_benchmark(&block_context);

                let executor_config = TransactionExecutorConfig::create_for_testing(false);
                let executor =
                    TransactionExecutor::new(state, block_context.clone(), executor_config);

                (tx, executor)
            },
            |(tx, mut executor)| {
                // Measured: execute the transaction.
                let results = executor.execute_txs(&[tx], None);
                let tx_execution_info = &results[0].as_ref().unwrap().0;
                assert!(
                    !tx_execution_info.is_reverted(),
                    "Transaction reverted: {:?}",
                    tx_execution_info.revert_error
                );
            },
            BatchSize::SmallInput,
        )
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(30);
    targets = transfers_benchmark,
             heavy_tx_benchmark_concurrent,
             heavy_tx_benchmark_sequential
}
criterion_main!(benches);
