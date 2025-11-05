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

use apollo_infra_utils::set_global_allocator;
use blockifier::blockifier::config::ConcurrencyConfig;
use blockifier::test_utils::transfers_generator::{
    RecipientGeneratorType,
    TransfersGenerator,
    TransfersGeneratorConfig,
};
#[cfg(feature = "cairo_native")]
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use criterion::{criterion_group, criterion_main, BatchSize, Criterion};

/// The name of the benchmark.
/// Differentiates between the benchmark running with the Cairo Native and the Cairo VM,
/// enabling proper comparison and regression tracking for each.
#[cfg(feature = "cairo_native")]
pub const BENCHMARK_NAME: &str = "transfers_benchmark_cairo_native";
#[cfg(not(feature = "cairo_native"))]
pub const BENCHMARK_NAME: &str = "transfers_benchmark_vm";

// TODO(Arni): Consider how to run this benchmark both with and without setting the allocator. Maybe
// hide this macro call under a feature, and run this benchmark regularly or with
// `cargo bench --bench blockifier --feature=specified_allocator`
set_global_allocator!();

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
    c.bench_function(BENCHMARK_NAME, |benchmark| {
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
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(50);
    targets = transfers_benchmark
}
criterion_main!(benches);
