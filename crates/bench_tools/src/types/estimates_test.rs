use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use apollo_infra_utils::path::project_path;
use rstest::{fixture, rstest};

use crate::test_utils::bench_tools_crate_dir;
use crate::types::estimates::{Estimates, GithubBenchmarkEntry, NS_PER_MS};

/// Returns the directory where dummy benchmark estimate results are stored.
#[fixture]
fn dummy_bench_results_dir(bench_tools_crate_dir: PathBuf) -> PathBuf {
    bench_tools_crate_dir.join("data/dummy_benches_result")
}

/// Returns the workspace root directory.
#[fixture]
fn workspace_root() -> PathBuf {
    project_path().expect("Failed to get project path")
}

///  Returns the list of dummy benchmark names.
#[fixture]
fn dummy_bench_names() -> &'static [&'static str] {
    &["dummy_sum_100", "dummy_sum_1000"]
}

// Helper function to deserialize Estimates from a dummy bench result file.
fn deserialize_dummy_bench_estimate_from_file(results_dir: &Path, bench_name: &str) -> Estimates {
    let path = results_dir.join(format!("{}_estimates.json", bench_name));
    let data = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {e}", path.display()));
    let estimates: Estimates = serde_json::from_str(&data).unwrap_or_else(|e| {
        panic!("Failed to deserialize {}: {e}\nContent: {data}", path.display())
    });
    estimates
}

#[rstest]
#[ignore]
/// Run dummy benchmark and deserialize the results.
fn run_dummy_bench_and_deserialize_estimates(
    workspace_root: PathBuf,
    dummy_bench_results_dir: PathBuf,
    dummy_bench_names: &[&str],
) {
    // 1) Run dummy benchmark.
    let status = Command::new("cargo")
        .args(["bench", "-p", "bench_tools", "--bench", "dummy_bench"])
        .status()
        .expect("Failed to spawn `cargo bench`");
    assert!(status.success(), "`cargo bench` did not exit successfully");

    // 2) Collect and save dummy_bench estimates.json files
    fs::create_dir_all(&dummy_bench_results_dir).expect("Failed to create results directory");

    for bench_name in dummy_bench_names {
        let source_path =
            workspace_root.join("target/criterion").join(bench_name).join("new/estimates.json");
        let dest_path = dummy_bench_results_dir.join(format!("{}_estimates.json", bench_name));

        // Read, parse, and write the result to the results directory.
        let data = fs::read_to_string(&source_path)
            .unwrap_or_else(|e| panic!("Failed to read {}: {}", source_path.display(), e));
        let json: serde_json::Value = serde_json::from_str(&data).expect("Failed to parse JSON");
        let pretty_json = serde_json::to_string_pretty(&json).expect("Failed to serialize JSON");
        fs::write(&dest_path, pretty_json).expect("Failed to write benchmark result");

        // 3) Validate deserialization of the saved results.
        deserialize_dummy_bench_estimate_from_file(&dummy_bench_results_dir, bench_name);
    }
}

#[rstest]
/// Test that Estimates can be deserialized from the saved results.
fn deserialize_dummy_bench_estimates(dummy_bench_results_dir: PathBuf, dummy_bench_names: &[&str]) {
    for bench_name in dummy_bench_names {
        deserialize_dummy_bench_estimate_from_file(&dummy_bench_results_dir, bench_name);
    }
}

#[rstest]
fn github_benchmark_entry_from_estimates(
    dummy_bench_results_dir: PathBuf,
    dummy_bench_names: &[&str],
) {
    for bench_name in dummy_bench_names {
        let estimates =
            deserialize_dummy_bench_estimate_from_file(&dummy_bench_results_dir, bench_name);
        let entry = GithubBenchmarkEntry::from_estimates(bench_name, &estimates);

        let expected_ms = estimates.mean.point_estimate / NS_PER_MS;
        assert_eq!(
            entry,
            GithubBenchmarkEntry {
                name: bench_name.to_string(),
                unit: "ms".to_string(),
                value: expected_ms
            }
        );
    }
}
