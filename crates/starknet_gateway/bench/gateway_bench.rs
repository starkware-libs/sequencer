//! Benchmark module for the starknet gateway crate. It provides functionalities to benchmark
//! the performance of the gateway service, including declare, deploy account and invoke
//! transactions.
//!
//! There are four benchmark functions in this flow: `declare_benchmark`,
//! `deploy_account_benchmark`, `invoke_benchmark` and `gateway_benchmark` which combines all of the
//! types. Each of the functions measure the performance of the gateway handling randomly created
//! txs of the respective type.
//!
//! Run the benchmarks using `cargo bench --bench gateway_bench`.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use starknet_gateway::bench_test_utils::{BenchTestSetup, BenchTestSetupConfig};

pub fn declare_benchmark(c: &mut Criterion) {
    c.bench_function("declares", |benchmark| benchmark.iter(|| {}));
}

pub fn deploy_account_benchmark(c: &mut Criterion) {
    c.bench_function("deploy_accounts", |benchmark| benchmark.iter(|| {}));
}

pub fn invoke_benchmark(c: &mut Criterion) {
    let tx_generator_config = BenchTestSetupConfig::default();
    let n_txs = tx_generator_config.n_txs;

    let test_setup = BenchTestSetup::new(tx_generator_config);
    c.bench_with_input(BenchmarkId::new("invoke", n_txs), &test_setup, |b, s| {
        b.to_async(tokio::runtime::Runtime::new().unwrap()).iter(|| s.send_txs_to_gateway());
    });
}

pub fn gateway_benchmark(c: &mut Criterion) {
    c.bench_function("all_transaction_types", |benchmark| benchmark.iter(|| {}));
}

criterion_group!(
    benches,
    // declare_benchmark,
    // deploy_account_benchmark,
    invoke_benchmark,
    // gateway_benchmark
);
criterion_main!(benches);
