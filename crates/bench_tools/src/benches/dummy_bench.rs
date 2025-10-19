use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use serde::{Deserialize, Serialize};

// Input files are embedded at compile time.
// Before building, ensure these files exist (e.g., downloaded from GCS).
const SMALL_INPUT: &str = include_str!("../../data/dummy_bench_input/small_input.json");
const LARGE_INPUT: &str = include_str!("../../data/dummy_bench_input/large_input.json");

#[derive(Debug, Serialize, Deserialize)]
struct DummyInput {
    values: Vec<u64>,
    multiplier: u64,
}

#[allow(dead_code)]
fn dummy_function(n: u64) -> u64 {
    // Simple function that does some work
    (0..n).sum()
}

/// Example benchmark functions to demonstrate how to use Criterion for benchmarking.
/// This is used to test the benchmarking infrastructure and generate sample benchmark results
/// that can be parsed by the bench_tools framework.
#[allow(dead_code)]
fn dummy_benchmark(c: &mut Criterion) {
    // black_box prevents the compiler from optimizing away the function call during benchmarking
    c.bench_function("dummy_sum_100", |b| b.iter(|| black_box(dummy_function(100))));

    c.bench_function("dummy_sum_1000", |b| b.iter(|| black_box(dummy_function(1000))));
}

#[allow(dead_code)]
fn dummy_benchmark_with_input(c: &mut Criterion) {
    let process_input = |input: &DummyInput| input.values.iter().sum::<u64>() * input.multiplier;

    let small_input: DummyInput = serde_json::from_str(SMALL_INPUT).unwrap();
    let large_input: DummyInput = serde_json::from_str(LARGE_INPUT).unwrap();

    c.bench_function("dummy_process_small_input", |b| {
        // black_box prevents the compiler from optimizing away the function call during
        // benchmarking
        b.iter(|| black_box(process_input(&small_input)))
    });

    c.bench_function("dummy_process_large_input", |b| {
        b.iter(||black_box( process_input(&large_input)))
    });
}

criterion_group!(benches, dummy_benchmark, dummy_benchmark_with_input);
criterion_main!(benches);
