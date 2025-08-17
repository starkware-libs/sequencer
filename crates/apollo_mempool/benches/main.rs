//! Benchmark module for the starknet mempool crate. It provides functionalities to benchmark
//! the performance of the mempool service with invoke transactions.
//!
//! There are one benchmark functions in this flow: `invoke_benchmark`. This functions measure the
//! performance of the mempool handling randomly created txs of the respective type.
//!
//! Run the benchmarks using `cargo bench --bench apollo_mempool`.

/// import the Mempool test utilities.
mod utils;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use utils::{BenchTestSetup, BenchTestSetupConfig};

fn run_benchmark(criterion: &mut Criterion, config: &BenchTestSetupConfig, fn_name: &str) {
    let test_setup = BenchTestSetup::new(config);
    let id_param = format!("{}_{}_1", config.n_txs, config.add_to_get_ratio);
    criterion.bench_with_input(
        BenchmarkId::new(fn_name, id_param),
        &test_setup,
        |bencher, test_setup| {
            bencher
                .to_async(tokio::runtime::Runtime::new().unwrap())
                .iter(|| test_setup.mempool_add_get_txs());
        },
    );
}

fn invoke_benchmarks(criterion: &mut Criterion) {
    let configs = [
        BenchTestSetupConfig::default(),
        BenchTestSetupConfig { add_to_get_ratio: 10, ..BenchTestSetupConfig::default() },
        BenchTestSetupConfig { add_to_get_ratio: 20, ..BenchTestSetupConfig::default() },
        BenchTestSetupConfig { n_txs: 200, add_to_get_ratio: 1, ..BenchTestSetupConfig::default() },
        BenchTestSetupConfig {
            n_txs: 200,
            add_to_get_ratio: 10,
            ..BenchTestSetupConfig::default()
        },
        BenchTestSetupConfig {
            n_txs: 200,
            add_to_get_ratio: 20,
            ..BenchTestSetupConfig::default()
        },
    ];

    for config in configs.iter() {
        run_benchmark(criterion, config, "invoke");
    }
}

criterion_group!(benches, invoke_benchmarks);
criterion_main!(benches);
