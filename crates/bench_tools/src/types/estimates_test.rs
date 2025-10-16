use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use rstest::{fixture, rstest};

use crate::types::estimates::Estimates;

/// Test fixture: Returns the bench_tools crate directory.
#[fixture]
fn manifest_dir() -> PathBuf {
    std::env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().unwrap())
}

/// Returns the data directory of the crate.
#[fixture]
fn data_dir(manifest_dir: PathBuf) -> PathBuf {
    manifest_dir.join("data/dummy_benches_result")
}

/// Returns the workspace root directory (two levels up from the crate).
#[fixture]
fn workspace_root(manifest_dir: PathBuf) -> PathBuf {
    manifest_dir.parent().unwrap().parent().unwrap().to_path_buf()
}

/// Helper function to deserialize dummy bench estimates JSON files in a directory.
fn assert_deserialize_dummy_bench_estimates(data_dir: &Path) {
    // Collect dummy benchmark estimate files.
    let bench_names = vec!["dummy_sum_100", "dummy_sum_1000"];
    let mut files: Vec<PathBuf> = Vec::new();

    for bench_name in bench_names {
        let path = data_dir.join(format!("{}_estimates.json", bench_name));
        if path.exists() {
            files.push(path);
        }
    }

    assert!(!files.is_empty(), "No dummy benchmark estimate files found in {}", data_dir.display());

    // Deserialize each file in the data directory.
    for path in &files {
        let data = fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("Failed to read {}: {}", path.display(), e));

        let _est: Estimates = serde_json::from_str(&data).unwrap_or_else(|e| {
            panic!("Failed to deserialize {}: {}\nContent: {}", path.display(), e, data)
        });
    }
}

#[rstest]
#[ignore]
/// Run dummy benchmark and deserialize the results.
fn run_dummy_bench_and_deserialize_estimates(workspace_root: PathBuf, data_dir: PathBuf) {
    // 1) Run dummy benchmark.
    let status = Command::new("cargo")
        .args(["bench", "-p", "bench_tools", "--bench", "dummy_bench"])
        .status()
        .expect("Failed to spawn `cargo bench`");
    assert!(status.success(), "`cargo bench` did not exit successfully");

    // 2) Collect only the dummy_bench estimates.json files
    let bench_names = vec!["dummy_sum_100", "dummy_sum_1000"];
    let mut files: Vec<PathBuf> = Vec::new();

    for bench_name in bench_names {
        let path =
            workspace_root.join("target/criterion").join(bench_name).join("new/estimates.json");
        if path.exists() {
            files.push(path);
        }
    }

    assert!(!files.is_empty(), "No dummy_bench results found; did the benchmark run successfully?");

    // 3) Save results to bench_tools/data.
    fs::create_dir_all(&data_dir).expect("Failed to create data directory");
    for path in &files {
        if let Some(filename) = path.file_name() {
            let bench_name = path
                .parent()
                .and_then(|p| p.parent())
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");
            let dest = data_dir.join(format!("{}_{}", bench_name, filename.to_str().unwrap()));

            // Read, parse, and write the result to the data directory.
            let data = fs::read_to_string(path).expect("Failed to read benchmark result");
            let json: serde_json::Value =
                serde_json::from_str(&data).expect("Failed to parse JSON");
            let pretty_json =
                serde_json::to_string_pretty(&json).expect("Failed to serialize JSON");
            fs::write(&dest, pretty_json).expect("Failed to write benchmark result");
        }
    }

    // 4) Deserialize and validate the saved results
    assert_deserialize_dummy_bench_estimates(&data_dir);
}

#[rstest]
/// Test that Estimates can be deserialized from the saved results.
fn deserialize_dummy_bench_estimates(data_dir: PathBuf) {
    assert_deserialize_dummy_bench_estimates(&data_dir);
}
