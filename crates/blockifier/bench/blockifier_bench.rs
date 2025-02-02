//! Benchmark module for the blockifier crate. It provides functionalities to benchmark
//! various aspects related to transferring between accounts, including preparation
//! and execution of transfers.
//!
//! The main benchmark function is `transfers_benchmark`, which measures the performance
//! of transfers between randomly created accounts, which are iterated over round-robin.
//!
//! Run the benchmarks using `cargo bench --bench blockifier_bench`.

use blockifier::blockifier::config::ConcurrencyConfig;
use blockifier::test_utils::transfers_generator::{
    RecipientGeneratorType,
    TransfersGenerator,
    TransfersGeneratorConfig,
};
use criterion::{criterion_group, criterion_main, Criterion};

pub fn transfers_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("transfers");
    let mut bench_scenarios = vec![];

    let id = "with concurrency";
    let concurrency_enabled = true;
    let transfers_generator_config = TransfersGeneratorConfig {
        recipient_generator_type: RecipientGeneratorType::Random,
        concurrency_config: ConcurrencyConfig::create_for_testing(concurrency_enabled),
        ..Default::default()
    };
    bench_scenarios.push((id, transfers_generator_config.clone()));

    let id = "without concurrency";
    let concurrency_enabled = false;
    let transfers_generator_config = TransfersGeneratorConfig {
        concurrency_config: ConcurrencyConfig::create_for_testing(concurrency_enabled),
        ..transfers_generator_config
    };
    bench_scenarios.push((id, transfers_generator_config));

    for (id, transfers_generator_config) in bench_scenarios {
        let mut transfers_generator = TransfersGenerator::new(transfers_generator_config);
        // Create a benchmark group, which iterates over the accounts round-robin and performs
        // transfers.
        group.bench_function(id, |benchmark| {
            benchmark.iter(|| {
                transfers_generator.execute_transfers();
            })
        });
    }
}

criterion_group!(benches, transfers_benchmark);
criterion_main!(benches);
