//! Benchmark suite for the Apollo mempool crate.
//!
//! This module provides tools to measure the performance of the mempool service under various
//! transaction loads and configurations.
//!
//! The main benchmark, `invoke_benchmark`, evaluates how efficiently the mempool processes randomly
//! generated invoke transactions across different scenarios.
//!
//! To run the benchmarks, use: `cargo bench --bench apollo_mempool`.
/// import the Mempool test utilities.
mod utils;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use utils::{BenchTestSetup, BenchTestSetupConfig};

fn run_invoke_benchmark(criterion: &mut Criterion, config: &BenchTestSetupConfig) {
    let test_setup = BenchTestSetup::new(config);
    let id_param = format!("{}_{}_1", config.n_txs, config.chunk_size);
    criterion.bench_with_input(
        BenchmarkId::new("invoke", id_param),
        &test_setup,
        |bencher, test_setup| {
            bencher
                .to_async(tokio::runtime::Runtime::new().unwrap())
                .iter(|| test_setup.mempool_add_get_txs());
        },
    );
}

fn invoke_benchmarks(criterion: &mut Criterion) {
    let configs =
        [BenchTestSetupConfig { n_txs: 1000, chunk_size: 100, ..BenchTestSetupConfig::default() }];

    for config in configs.iter() {
        run_invoke_benchmark(criterion, config);
    }
}

criterion_group!(benches, invoke_benchmarks);
criterion_main!(benches);
