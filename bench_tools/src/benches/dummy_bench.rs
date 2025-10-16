use criterion::{black_box, criterion_group, criterion_main, Criterion};

#[allow(dead_code)]
fn dummy_function(n: u64) -> u64 {
    // Simple function that does some work
    (0..n).sum()
}

#[allow(dead_code)]
fn dummy_benchmark(c: &mut Criterion) {
    c.bench_function("dummy_sum_100", |b| b.iter(|| dummy_function(black_box(100))));

    c.bench_function("dummy_sum_1000", |b| b.iter(|| dummy_function(black_box(1000))));
}

criterion_group!(benches, dummy_benchmark);
criterion_main!(benches);
