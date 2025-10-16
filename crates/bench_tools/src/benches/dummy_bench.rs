use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};

#[allow(dead_code)]
fn dummy_function(n: u64) -> u64 {
    // Simple function that does some work
    (0..n).sum()
}

/// Example benchmark function that demonstrates how to use Criterion for benchmarking.
/// This is used to test the benchmarking infrastructure and generate sample benchmark results
/// that can be parsed by the bench_tools framework.
#[allow(dead_code)]
fn dummy_benchmark(c: &mut Criterion) {
    // black_box prevents the compiler from optimizing away the function call during benchmarking
    c.bench_function("dummy_sum_100", |b| b.iter(|| black_box(dummy_function(100))));

    c.bench_function("dummy_sum_1000", |b| b.iter(|| black_box(dummy_function(1000))));
}

criterion_group!(benches, dummy_benchmark);
criterion_main!(benches);
