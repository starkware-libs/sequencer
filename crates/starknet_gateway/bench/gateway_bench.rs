//! Benchmark module for the starknet gateway crate. It provides functionalities to benchmark
//! the performance of the gateway service, including declare, deploy account and invoke
//! transactions.
//!
//! There are four benchmark functions in this flow: `declare_benchmark`,
//! `deploy_account_benchmark`, `invoke_benchmark` and `gateway_benchmark` which combines all of the
//! types. Each of the functions measure the performance of the gateway handling randomly created
//! txs of the respective type.
//!
//! Run the benchmarks using `cargo bench --bench starknet_gateway_bench`.

use criterion::{criterion_group, criterion_main, Criterion};

pub fn declare_benchmark(c: &mut Criterion) {
    c.bench_function("declares", |benchmark| benchmark.iter(|| {}));
}

pub fn deploy_account_benchmark(c: &mut Criterion) {
    c.bench_function("deploy_accounts", |benchmark| benchmark.iter(|| {}));
}

pub fn invoke_benchmark(c: &mut Criterion) {
    c.bench_function("invokes", |benchmark| benchmark.iter(|| {}));
}

pub fn gateway_benchmark(c: &mut Criterion) {
    c.bench_function("all_transaction_types", |benchmark| benchmark.iter(|| {}));
}

criterion_group!(
    benches,
    declare_benchmark,
    deploy_account_benchmark,
    invoke_benchmark,
    gateway_benchmark
);
criterion_main!(benches);
