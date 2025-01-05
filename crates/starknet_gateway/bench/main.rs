mod bench_lib;

use bench_lib::utils::{BenchTestSetup, BenchTestSetupConfig};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};

fn invoke_benchmark(criterion: &mut Criterion) {
    let tx_generator_config = BenchTestSetupConfig::default();
    let n_txs = tx_generator_config.n_txs;

    let test_setup = BenchTestSetup::new(tx_generator_config);
    criterion.bench_with_input(
        BenchmarkId::new("invoke", n_txs),
        &test_setup,
        |bencher, test_setup| {
            bencher
                .to_async(tokio::runtime::Runtime::new().unwrap())
                .iter(|| test_setup.send_txs_to_gateway());
        },
    );
}

criterion_group!(benches, invoke_benchmark);
criterion_main!(benches);
