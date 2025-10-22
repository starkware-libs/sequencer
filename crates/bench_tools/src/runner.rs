use std::fs;
use std::path::PathBuf;

use crate::gcs;
use crate::types::benchmark_config::BenchmarkConfig;
use crate::utils::copy_dir_contents;

/// Prepares inputs for a benchmark.
/// If the benchmark needs inputs and a local input directory is provided,
/// it copies the contents from the local directory to the expected input location.
/// If the benchmark needs inputs and no local input directory is provided,
/// it downloads the inputs from GCS.
fn prepare_inputs(bench: &BenchmarkConfig, input_dir: Option<&str>) {
    if !bench.needs_inputs() {
        return;
    }

    let benchmark_input_dir = PathBuf::from(bench.input_dir.unwrap());

    // Create the input directory if it doesn't exist.
    fs::create_dir_all(&benchmark_input_dir).unwrap_or_else(|e| {
        panic!("Failed to create directory {}: {}", benchmark_input_dir.display(), e)
    });

    if let Some(local_dir) = input_dir {
        let local_path = PathBuf::from(local_dir);
        if !local_path.exists() {
            panic!("Input directory does not exist: {}", local_dir);
        }

        // Copy local directory contents to the benchmark input directory.
        copy_dir_contents(&local_path, &benchmark_input_dir);

        println!("Copied inputs from {} to {}", local_dir, benchmark_input_dir.display());
    } else {
        gcs::download_inputs(bench.name, &benchmark_input_dir);
        if !benchmark_input_dir.exists() {
            panic!(
                "Failed to download inputs for {}: {}",
                bench.name,
                benchmark_input_dir.display()
            );
        }
    }
}

/// Runs a single benchmark and panic if it fails.
fn run_single_benchmark(bench: &BenchmarkConfig) {
    println!("Running: {}", bench.name);

    let output = std::process::Command::new("cargo")
        .args(bench.cmd_args)
        .output()
        .unwrap_or_else(|e| panic!("Failed to execute {}: {}", bench.name, e));

    if !output.status.success() {
        panic!("\nBenchmark {} failed:\n{}", bench.name, String::from_utf8_lossy(&output.stderr));
    }
}

/// Collects benchmark results from criterion output and saves them to the output directory.
fn save_benchmark_results(bench: &BenchmarkConfig, output_dir: &str) {
    let criterion_base = PathBuf::from("target/criterion");

    // Get the list of criterion benchmark names to save.
    // If None, use the benchmark config name.
    let benchmark_names: Vec<&str> = match bench.criterion_benchmark_names {
        Some(names) => names.to_vec(),
        None => vec![bench.name],
    };

    // Save results for each criterion benchmark name.
    for bench_name in benchmark_names {
        let bench_path = criterion_base.join(bench_name);
        let estimates_path = bench_path.join("new/estimates.json");

        if let Ok(data) = fs::read_to_string(&estimates_path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&data) {
                if let Ok(pretty) = serde_json::to_string_pretty(&json) {
                    let dest =
                        PathBuf::from(output_dir).join(format!("{}_estimates.json", bench_name));
                    if fs::write(&dest, pretty).is_ok() {
                        println!("Saved results: {}", dest.display());
                    }
                }
            }
        }
    }
}

/// Runs benchmarks for a given package, handling input downloads if needed.
pub fn run_benchmarks(benchmarks: &[&BenchmarkConfig], input_dir: Option<&str>, output_dir: &str) {
    // Prepare inputs.
    for bench in benchmarks {
        prepare_inputs(bench, input_dir);
    }

    // Create output directory.
    fs::create_dir_all(output_dir).unwrap_or_else(|e| panic!("Failed to create output dir: {}", e));

    // Run benchmarks.
    for bench in benchmarks {
        run_single_benchmark(bench);
        save_benchmark_results(bench, output_dir);
    }

    println!("\n✓ All benchmarks completed! Results saved to: {}", output_dir);
}

/// Runs benchmarks and compares them against previous results, failing if regression exceeds limit.
pub async fn run_and_compare_benchmarks(
    benchmarks: &[&BenchmarkConfig],
    input_dir: Option<&str>,
    output_dir: &str,
    regression_limit: f64,
) {
    // Run benchmarks first.
    run_benchmarks(benchmarks, input_dir, output_dir);

    // Collect all criterion benchmark names from configs.
    let mut bench_names = Vec::new();
    for bench in benchmarks {
        bench_names.extend(bench.criterion_benchmark_names.unwrap_or(&[bench.name]));
    }

    println!("\n📊 Checking for performance regressions (limit: {}%):", regression_limit);
    let regression_result = crate::comparison::check_regressions(&bench_names, regression_limit);

    match regression_result {
        Ok(_) => {
            println!("\n✅ All benchmarks passed regression check!");
        }
        Err((error_msg, results)) => {
            // Some benchmarks exceeded the limit - print detailed results.
            println!("\nBenchmark Results:");
            for result in results {
                if result.exceeds_limit {
                    println!(
                        "  ❌ {}: {:+.2}% (EXCEEDS {:.1}% limit)",
                        result.name, result.change_percentage, regression_limit
                    );
                } else {
                    println!(
                        "  ✓ {}: {:+.2}% (within {:.1}% limit)",
                        result.name, result.change_percentage, regression_limit
                    );
                }
            }
            panic!("\n{}", error_msg);
        }
    }
}
