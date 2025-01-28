//! Placeholder similar to `crates/blockifier/bench/blockifier_bench.rs`.
//!
//! Run the benchmarks using `cargo bench --bench starknet_batcher_bench`.

use blockifier::test_utils::transfers_generator::{
    RecipientGeneratorType,
    TransfersGeneratorConfig,
};
use criterion::{criterion_group, criterion_main, Criterion};
use starknet_batcher::bench_utils::TransfersGenerator;

pub fn transfers_benchmark(c: &mut Criterion) {
    let transfers_generator_config = TransfersGeneratorConfig {
        recipient_generator_type: RecipientGeneratorType::Random,
        ..Default::default()
    };
    let mut transfers_generator = TransfersGenerator::new(transfers_generator_config);
    // Create a benchmark group called "transfers", which iterates over the accounts round-robin
    // and performs transfers.
    c.bench_function("transfers", |benchmark| {
        benchmark.iter(|| {
            transfers_generator.execute_transfers();
        })
    });
}

criterion_group!(benches, transfers_benchmark);
criterion_main!(benches);
