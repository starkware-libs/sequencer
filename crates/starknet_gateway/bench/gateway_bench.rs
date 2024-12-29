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

use criterion::{criterion_group, criterion_main, Criterion};

fn invoke_benchmark(criterion: &mut Criterion) {
    criterion.bench_function("invokes", |benchmark| benchmark.iter(|| {}));
}

criterion_group!(benches, invoke_benchmark);
criterion_main!(benches);
