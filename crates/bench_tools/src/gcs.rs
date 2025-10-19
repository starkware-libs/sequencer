use std::path::Path;

/// Default GCS bucket for benchmarks.
pub const BENCHMARKS_BUCKET: &str = "benchmarks_ci";

/// Uploads all files from a local directory to Google Cloud Storage.
///
/// Uses gcloud CLI to upload files. Before running, authenticate with:
/// `gcloud auth application-default login`
///
/// Files are uploaded to: `gs://{BENCHMARKS_BUCKET}/{benchmark_name}/input/`
pub async fn upload_inputs(benchmark_name: &str, input_dir: &Path) {
    println!(
        "Uploading inputs from {} to gs://{}/{}/input/",
        input_dir.display(),
        BENCHMARKS_BUCKET,
        benchmark_name
    );

    let source = format!("{}/*", input_dir.display());
    let dest = format!("gs://{}/{}/input/", BENCHMARKS_BUCKET, benchmark_name);

    // Use gcloud storage cp command to upload files.
    let output = tokio::process::Command::new("gcloud")
        .args(["storage", "cp", "-r", &source, &dest])
        .output()
        .await
        .expect("Failed to upload inputs to GCS");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!("Failed to upload inputs to GCS: {}", stderr);
    }

    println!("{}", String::from_utf8_lossy(&output.stdout).trim());
    println!("Input files uploaded successfully!");
}

/// Downloads all input files for a benchmark from Google Cloud Storage.
///
/// Uses gcloud CLI to download files. Before running, authenticate with:
/// `gcloud auth application-default login`
///
/// Downloads from: `gs://{BENCHMARKS_BUCKET}/{benchmark_name}/input/` to the local input directory.
pub async fn download_inputs(benchmark_name: &str, local_input_dir: &Path) {
    println!(
        "Downloading inputs from gs://{}/{}/input/ to {}",
        BENCHMARKS_BUCKET,
        benchmark_name,
        local_input_dir.display()
    );

    let source = format!("gs://{}/{}/input/*", BENCHMARKS_BUCKET, benchmark_name);
    let dest = local_input_dir.display().to_string();

    // Use gcloud storage cp command to download files.
    let output = tokio::process::Command::new("gcloud")
        .args(["storage", "cp", "-r", &source, &dest])
        .output()
        .await
        .expect("Failed to cp inputs from GCS");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!("Failed to download inputs from GCS: {}", stderr);
    }

    println!("{}", String::from_utf8_lossy(&output.stdout).trim());
    println!("Input files downloaded successfully!");
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tempfile::TempDir;
    use tokio::fs;

    use super::*;

    #[tokio::test]
    #[ignore] // Run with: cargo test -p bench_tools -- --ignored
    async fn test_upload_and_download_inputs() {
        let benchmark_name = "dummy_benchmark";

        // Get paths relative to workspace root.
        let workspace_root = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let source_dir = PathBuf::from(&workspace_root).join("data/dummy_bench_input");

        // Ensure source files exist.
        assert!(source_dir.exists(), "Source directory does not exist: {}", source_dir.display());

        // Upload inputs.
        println!("Testing upload...");
        upload_inputs(benchmark_name, &source_dir).await;

        // Create temp directory for download.
        let temp_dir = TempDir::new().unwrap();
        let download_dir = temp_dir.path();

        println!("\nDownload directory: {}", download_dir.display());

        // Verify temp directory is empty initially.
        assert!(
            fs::read_dir(download_dir).await.unwrap().next_entry().await.unwrap().is_none(),
            "Temp directory should be empty initially"
        );

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
}
