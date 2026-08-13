use std::collections::HashMap;

use crate::runner::run_and_compare_benchmarks;
use crate::types::benchmark_config::BenchmarkConfig;

/// Exits non-zero if run.
const FAILING_BENCHMARK: BenchmarkConfig = BenchmarkConfig {
    name: "known_benchmark",
    package: "bench_tools",
    cmd_args: &["--not-a-cargo-flag"],
    input_dir: None,
    criterion_benchmark_names: None,
};

#[test]
#[should_panic(expected = "Absolute time limits name unknown benchmarks: {\"unknown_benchmark\"}")]
fn unknown_absolute_limit_name_is_rejected_before_running_benchmarks() {
    let output_dir = tempfile::tempdir().unwrap();

    run_and_compare_benchmarks(
        &[&FAILING_BENCHMARK],
        None,
        output_dir.path().to_str().unwrap(),
        8.0,
        HashMap::from([("unknown_benchmark".to_string(), 1.0)]),
    );
}
