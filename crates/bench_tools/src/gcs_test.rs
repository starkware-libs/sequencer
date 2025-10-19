use tempfile::TempDir;
use tokio::fs;

use crate::gcs::{download_inputs, upload_inputs};
use crate::test_utils;

#[tokio::test]
#[ignore] // Run with: cargo test -p bench_tools -- --ignored
async fn test_upload_and_download_inputs() {
    let benchmark_name = "dummy_benchmark";

    // Get paths relative to bench_tools crate directory.
    let source_dir = test_utils::bench_tools_crate_dir().join("data/dummy_bench_input");

    // Ensure source files exist.
    assert!(source_dir.exists(), "Source directory does not exist: {}", source_dir.display());

    // Upload inputs.
    println!("Testing upload...");
    upload_inputs(benchmark_name, &source_dir).await;

    // Create temp directory for download.
    let temp_dir = TempDir::new().unwrap();
    let download_dir = temp_dir.path();

    println!("\nDownload directory: {}", download_dir.display());

    // Download inputs to temp directory.
    println!("\nTesting download...");
    download_inputs(benchmark_name, download_dir).await;

    // Verify files were downloaded.
    let small_input = download_dir.join("small_input.json");
    let large_input = download_dir.join("large_input.json");

    assert!(small_input.exists(), "small_input.json was not downloaded");
    assert!(large_input.exists(), "large_input.json was not downloaded");

    // Verify content matches original.
    let original_small = fs::read_to_string(source_dir.join("small_input.json")).await.unwrap();
    let downloaded_small = fs::read_to_string(&small_input).await.unwrap();
    assert_eq!(original_small, downloaded_small, "small_input.json content does not match");

    let original_large = fs::read_to_string(source_dir.join("large_input.json")).await.unwrap();
    let downloaded_large = fs::read_to_string(&large_input).await.unwrap();
    assert_eq!(original_large, downloaded_large, "large_input.json content does not match");
}
