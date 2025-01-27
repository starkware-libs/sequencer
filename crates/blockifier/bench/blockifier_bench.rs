//! Benchmark module for the blockifier crate. It provides functionalities to benchmark
//! various aspects related to transferring between accounts, including preparation
//! and execution of transfers.
//!
//! The main benchmark function is `transfers_benchmark`, which measures the performance
//! of transfers between randomly created accounts, which are iterated over round-robin.
//!
//! Run the benchmarks using `cargo bench --bench blockifier_bench`.

use blockifier::test_utils::transfers_generator::{
    RecipientGeneratorType,
    TransfersGenerator,
    TransfersGeneratorConfig,
};
use criterion::{criterion_group, criterion_main, Criterion};

pub fn transfers_benchmark(c: &mut Criterion) {
    // Request jemalloc statistics
    let mut buf = [0u8; 1024];
    let stats = unsafe {
        tikv_jemalloc_sys::mallctl(
            b"stats.print\0".as_ptr() as *const _,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            buf.as_mut_ptr() as *mut _,
            buf.len(),
        )
    };

    if stats == 0 {
        println!("Jemalloc stats:\n{}", String::from_utf8_lossy(&buf));
    } else {
        eprintln!("Failed to fetch jemalloc stats");
    }

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
