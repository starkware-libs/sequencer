use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::gcs;
use crate::types::benchmark_config::BenchmarkConfig;
use crate::types::estimates::{Estimates, GithubBenchmarkEntry};
use crate::utils::copy_dir_contents;

/// Default output directory used by the Criterion benchmarking library.
const CRITERION_OUTPUT_DIR: &str = "target/criterion";

/// Path to the estimates file within a Criterion benchmark directory.
/// Criterion stores the latest benchmark results in this subdirectory structure.
const CRITERION_ESTIMATES_PATH: &str = "new/estimates.json";

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
    let display_path = benchmark_input_dir.display();
    fs::create_dir_all(&benchmark_input_dir)
        .unwrap_or_else(|e| panic!("Failed to create directory {display_path}: {e}"));

    if let Some(local_dir) = input_dir {
        let local_path = PathBuf::from(local_dir);
        if !local_path.exists() {
            panic!("Input directory does not exist: {local_dir}");
        }

        // Copy local directory contents to the benchmark input directory.
        copy_dir_contents(&local_path, &benchmark_input_dir);

        let display_path = benchmark_input_dir.display();
        println!("Copied inputs from {local_dir} to {display_path}");
    } else {
        gcs::download_inputs(bench.name, &benchmark_input_dir);
        if !benchmark_input_dir.exists() {
            let bench_name = bench.name;
            let display_path = benchmark_input_dir.display();
            panic!("Failed to download inputs for {bench_name}: {display_path}");
        }
    }
}

/// Runs a single benchmark and panic if it fails.
fn run_single_benchmark(bench: &BenchmarkConfig) {
    let bench_name = bench.name;
    println!("Running: {bench_name}");

    let status = std::process::Command::new("cargo")
        .args(bench.cmd_args)
        .status()
        .unwrap_or_else(|e| panic!("Failed to execute {bench_name}: {e}"));

    if !status.success() {
        let code = status.code();
        panic!("\nBenchmark {bench_name} failed with exit code: {code:?}");
    }
}

/// Collects benchmark results from criterion output and saves them to the output directory.
fn save_benchmark_results(bench: &BenchmarkConfig, output_dir: &str) {
    let criterion_base = PathBuf::from(CRITERION_OUTPUT_DIR);

    // Get the list of criterion benchmark names to save.
    // If None, use the benchmark config name.
    let benchmark_names: Vec<&str> = match bench.criterion_benchmark_names {
        Some(names) => names.to_vec(),
        None => vec![bench.name],
    };

    // Save results for each criterion benchmark name.
    for bench_name in benchmark_names {
        let bench_path = criterion_base.join(bench_name);
        let estimates_path = bench_path.join(CRITERION_ESTIMATES_PATH);

        if let Ok(data) = fs::read_to_string(&estimates_path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&data) {
                if let Ok(pretty) = serde_json::to_string_pretty(&json) {
                    let dest =
                        PathBuf::from(output_dir).join(format!("{bench_name}_estimates.json"));
                    if fs::write(&dest, pretty).is_ok() {
                        let display_path = dest.display();
                        println!("Saved results: {display_path}");
                    }
                }
            }
        }
    }
}

/// Runs benchmarks for a given package, handling input downloads if needed.
pub fn run_benchmarks(
    benchmarks: &[&BenchmarkConfig],
    input_dir: Option<&str>,
    output_dir: &str,
    github_action_benchmark_output_file: Option<&str>,
) {
    // Prepare inputs.
    for bench in benchmarks {
        prepare_inputs(bench, input_dir);
    }

    // Create output directory.
    fs::create_dir_all(output_dir).unwrap_or_else(|e| panic!("Failed to create output dir: {e}"));

    // Run benchmarks.
    for bench in benchmarks {
        run_single_benchmark(bench);
        save_benchmark_results(bench, output_dir);
    }

    // Generate github-action-benchmark output if requested.
    if let Some(output_file) = github_action_benchmark_output_file {
        write_github_action_benchmark_json(benchmarks, output_file);
    }

    println!("\n✓ All benchmarks completed! Results saved to: {output_dir}");
}

/// Generates JSON output in github-action-benchmark format.
/// Reads Criterion estimates and converts to the "customSmallerIsBetter" format.
fn write_github_action_benchmark_json(benchmarks: &[&BenchmarkConfig], output_file: &str) {
    let criterion_base = PathBuf::from(CRITERION_OUTPUT_DIR);
    let mut entries = Vec::new();

    for bench in benchmarks {
        let benchmark_names: Vec<&str> =
            bench.criterion_benchmark_names.map(|names| names.to_vec()).unwrap_or(vec![bench.name]);

        for bench_name in benchmark_names {
            let estimates_path = criterion_base.join(bench_name).join(CRITERION_ESTIMATES_PATH);
            let display_path = estimates_path.display();

            let data = fs::read_to_string(&estimates_path)
                .unwrap_or_else(|e| panic!("Failed to read {display_path}: {e}"));

            let estimates: Estimates = serde_json::from_str(&data)
                .unwrap_or_else(|e| panic!("Failed to parse {display_path} as JSON: {e}"));

            entries.push(GithubBenchmarkEntry::from_estimates(bench_name, &estimates));
        }
    }

    let json = serde_json::to_string_pretty(&entries)
        .unwrap_or_else(|e| panic!("Failed to serialize benchmark results: {e}"));

    fs::write(output_file, json).unwrap_or_else(|e| panic!("Failed to write {output_file}: {e}"));

    println!("Saved github-action-benchmark output: {output_file}");
}

/// Runs benchmarks and compares them against previous results, failing if regression exceeds limit.
pub fn run_and_compare_benchmarks(
    benchmarks: &[&BenchmarkConfig],
    input_dir: Option<&str>,
    output_dir: &str,
    regression_limit: f64,
    absolute_time_ns_limits: HashMap<String, f64>,
) {
    // Run benchmarks first (no github-action-benchmark output for comparison runs).
    run_benchmarks(benchmarks, input_dir, output_dir, None);

    // Collect all criterion benchmark names from configs.
    let mut bench_names = Vec::new();
    for bench in benchmarks {
        bench_names.extend(bench.criterion_benchmark_names.unwrap_or(&[bench.name]));
    }

    print!("\n📊 Checking for performance regressions (limit: {regression_limit}%");
    if !absolute_time_ns_limits.is_empty() {
        let count = absolute_time_ns_limits.len();
        print!(", {count} benchmark(s) with absolute time limits");
    }
    let regression_result = crate::comparison::check_regressions(
        &bench_names,
        regression_limit,
        &absolute_time_ns_limits,
    );

    match regression_result {
        Ok(_) => {
            println!("\n✅ All benchmarks passed regression check!");
        }
        Err((error_msg, results)) => {
            // Some benchmarks exceeded the limit - print detailed results.
            println!("\nBenchmark Results:");
            for result in results {
                if result.exceeds_regression_limit {
                    let name = &result.name;
                    let change = result.change_percentage;
                    println!(" ❌ {name}: {change:+.2}% (EXCEEDS {regression_limit:.1}% limit)");
                }

                if result.exceeds_absolute_limit {
                    if let Some(&limit) = absolute_time_ns_limits.get(&result.name) {
                        let name = &result.name;
                        let time = result.absolute_time_ns;
                        println!(" ❌ {name}: {time:.2}ns (EXCEEDS {limit:.0}ns limit)");
                    }
                }

                if !result.exceeds_regression_limit && !result.exceeds_absolute_limit {
                    let name = &result.name;
                    let change = result.change_percentage;
                    let time = result.absolute_time_ns;
                    println!("  ✓ {name}: {change:+.2}% | {time:.2}ns");
                }
            }
            panic!("\n{error_msg}");
        }
    }
}
