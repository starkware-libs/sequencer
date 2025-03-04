//! Benchmark module for the blockifier crate. It provides functionalities to benchmark
//! various aspects related to transferring between accounts, including preparation
//! and execution of transfers.
//!
//! The main benchmark function is `transfers_benchmark`, which measures the performance
//! of transfers between randomly created accounts, which are iterated over round-robin.
//!
<<<<<<< HEAD:crates/blockifier/benches/main.rs
//! Run the benchmarks using `cargo bench --bench blockifier`.
//!
//! For Cairo Native compilation run the benchmarks using:
//! `cargo bench --bench blockifier --features "cairo_native"`.
||||||| f39b2b272:crates/blockifier/bench/blockifier_bench.rs
//! Run the benchmarks using `cargo bench --bench blockifier_bench`.
=======
//! Run the benchmarks using `cargo bench --bench blockifier_bench`.
//!
//! For Cairo Native compilation run the benchmarks using:
//! `cargo bench --bench blockifier_bench --features "cairo_native"`.
>>>>>>> origin/main-v0.13.5:crates/blockifier/bench/blockifier_bench.rs

use blockifier::test_utils::transfers_generator::{
    RecipientGeneratorType,
    TransfersGenerator,
    TransfersGeneratorConfig,
};
<<<<<<< HEAD:crates/blockifier/benches/main.rs
#[cfg(feature = "cairo_native")]
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
||||||| f39b2b272:crates/blockifier/bench/blockifier_bench.rs
=======
#[cfg(feature = "cairo_native")]
use blockifier::test_utils::{CairoVersion, RunnableCairo1};
>>>>>>> origin/main-v0.13.5:crates/blockifier/bench/blockifier_bench.rs
use criterion::{criterion_group, criterion_main, Criterion};
use starknet_infra_utils::set_global_allocator;

// TODO(Arni): Consider how to run this benchmark both with and without setting the allocator. Maybe
// hide this macro call under a feature, and run this benchmark regularly or with
// `cargo bench --bench blockifier --feature=specified_allocator`
set_global_allocator!();

pub fn transfers_benchmark(c: &mut Criterion) {
    let transfers_generator_config = TransfersGeneratorConfig {
        recipient_generator_type: RecipientGeneratorType::Random,
        #[cfg(feature = "cairo_native")]
        cairo_version: CairoVersion::Cairo1(RunnableCairo1::Native),
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
