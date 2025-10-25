use std::path::Path;
use std::process::Command;

/// Default GCS bucket for benchmarks.
pub const BENCHMARKS_BUCKET: &str = "apollo_benchmarks";

/// Uploads all files from a local directory to Google Cloud Storage.
///
/// Uses gcloud CLI to upload files. Before running, authenticate with:
/// `gcloud auth application-default login`
///
/// Files are uploaded to: `gs://{BENCHMARKS_BUCKET}/{benchmark_name}/input/`
pub fn upload_inputs(benchmark_name: &str, input_dir: &Path) {
    println!(
        "Uploading inputs from {} to gs://{}/{}/input/",
        input_dir.display(),
        BENCHMARKS_BUCKET,
        benchmark_name
    );

    let source = format!("{}/*", input_dir.display());
    let dest = format!("gs://{}/{}/input/", BENCHMARKS_BUCKET, benchmark_name);

    // Use gcloud storage cp command to upload files.
    let output = Command::new("gcloud")
        .args(["storage", "cp", "-r", &source, &dest])
        .output()
        .expect("Failed to upload inputs to GCS");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!("Failed to upload inputs to GCS: {}", stderr);
    }

    println!("{}", String::from_utf8_lossy(&output.stdout).trim());
    println!("Input files uploaded successfully!");
}
