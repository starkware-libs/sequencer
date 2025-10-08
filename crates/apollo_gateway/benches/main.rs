//! Benchmark module for the starknet gateway crate. It provides functionalities to benchmark
//! the performance of the gateway service, including declare, deploy account and invoke
//! transactions.
//!
//! There are four benchmark functions in this flow: `declare_benchmark`,
//! `deploy_account_benchmark`, `invoke_benchmark` and `gateway_benchmark` which combines all of the
//! types. Each of the functions measure the performance of the gateway handling randomly created
//! txs of the respective type.
//!
//! Run the benchmarks using `CC=clang cargo bench --bench apollo_gateway`.

// import the Gateway test utilities.
mod utils;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use utils::{BenchTestSetup, BenchTestSetupConfig};

fn invoke_benchmark(criterion: &mut Criterion) {
    let tx_generator_config = BenchTestSetupConfig::default();
    let n_txs = tx_generator_config.n_txs;

    let test_setup = BenchTestSetup::new(tx_generator_config);
    criterion.bench_with_input(
        BenchmarkId::new("invoke", n_txs),
        &test_setup,
        |bencher, test_setup| {
            bencher
                .to_async(tokio::runtime::Runtime::new().unwrap())
                .iter(|| test_setup.send_txs_to_gateway());
        },
    );
}

criterion_group!(benches, invoke_benchmark);
criterion_main!(benches);
