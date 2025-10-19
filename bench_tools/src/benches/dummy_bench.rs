use criterion::{black_box, criterion_group, criterion_main, Criterion};
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

#[allow(dead_code)]
fn process_input(input: &DummyInput) -> u64 {
    input.values.iter().sum::<u64>() * input.multiplier
}

#[allow(dead_code)]
fn dummy_benchmark(c: &mut Criterion) {
    c.bench_function("dummy_sum_100", |b| b.iter(|| dummy_function(black_box(100))));

    c.bench_function("dummy_sum_1000", |b| b.iter(|| dummy_function(black_box(1000))));
}

#[allow(dead_code)]
fn dummy_benchmark_with_input(c: &mut Criterion) {
    let small_input: DummyInput = serde_json::from_str(SMALL_INPUT).unwrap();
    let large_input: DummyInput = serde_json::from_str(LARGE_INPUT).unwrap();

    c.bench_function("dummy_process_small_input", |b| {
        b.iter(|| process_input(black_box(&small_input)))
    });

    c.bench_function("dummy_process_large_input", |b| {
        b.iter(|| process_input(black_box(&large_input)))
    });
}

criterion_group!(benches, dummy_benchmark, dummy_benchmark_with_input);
criterion_main!(benches);
