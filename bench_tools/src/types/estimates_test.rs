use std::fs;
use std::path::PathBuf;
use std::process::Command;

use glob::glob;

use crate::types::estimates::Estimates;

#[test]
#[ignore]
/// Run dummy benchmark and deserialize the results.
fn run_dummy_bench_and_deserialize_all_estimates() {
    let workspace_root = std::env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().unwrap())
        .parent()
        .unwrap()
        .to_path_buf();

    let data_dir = workspace_root.join("bench_tools/data/dummy_benches_result");

    // 1) Run dummy benchmark.
    let status = Command::new("cargo")
        .args(["bench", "-p", "bench_tools", "--bench", "dummy_bench"])
        .status()
        .expect("Failed to spawn `cargo bench`");
    assert!(status.success(), "`cargo bench` did not exit successfully");

    // 2) Collect ALL estimates.json files under target/criterion/**/new/
    let patterns =
        vec!["target/criterion/**/new/estimates.json", "../target/criterion/**/new/estimates.json"];

    let mut files: Vec<PathBuf> = Vec::new();
    for pattern in &patterns {
        for path in
            glob(pattern).unwrap_or_else(|_| panic!("Invalid glob pattern: {}", pattern)).flatten()
        {
            files.push(path);
        }
    }
    assert!(!files.is_empty(), "No Criterion results found; did any benches run?");

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
    deserialize_estimates();
}

#[test]
/// Test that Estimates can be deserialized from the saved results.
fn deserialize_estimates() {
    let workspace_root = std::env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().unwrap())
        .parent()
        .unwrap()
        .to_path_buf();

    let data_dir = workspace_root.join("bench_tools/data/dummy_benches_result");

    // Collect all JSON files in the data directory
    let pattern = data_dir.join("*.json");
    let mut files: Vec<PathBuf> = Vec::new();

    for path in glob(pattern.to_str().unwrap()).expect("Invalid glob pattern").flatten() {
        files.push(path);
    }

    assert!(!files.is_empty(), "No JSON files found in {}", data_dir.display());

    // Deserialize each file in the data directory.
    for path in &files {
        let data = fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("Failed to read {}: {}", path.display(), e));

        let _est: Estimates = serde_json::from_str(&data).unwrap_or_else(|e| {
            panic!("Failed to deserialize {}: {}\nContent: {}", path.display(), e, data)
        });
    }
}
