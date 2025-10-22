use std::fs;
use std::path::PathBuf;

use crate::types::estimates::Estimates;

/// Result of a benchmark comparison.
#[derive(Debug)]
pub struct BenchmarkComparison {
    pub name: String,
    pub change_percentage: f64,
    pub exceeds_limit: bool,
}

/// Loads change estimates from criterion's change directory for a given benchmark.
/// Panics if the change file doesn't exist.
fn load_change_estimates(bench_name: &str) -> Estimates {
    let change_path =
        PathBuf::from("target/criterion").join(bench_name).join("change/estimates.json");

    if !change_path.exists() {
        panic!(
            "Change file not found for benchmark '{}': {}\nThis likely means no baseline exists. \
             Run the benchmark at least once before using run-and-compare.",
            bench_name,
            change_path.display()
        );
    }

    let data = fs::read_to_string(&change_path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", change_path.display(), e));

    serde_json::from_str(&data).unwrap_or_else(|e| {
        panic!("Failed to deserialize {}: {}\nContent: {}", change_path.display(), e, data)
    })
}

/// Converts change estimates to percentage.
/// The mean.point_estimate in change/estimates.json represents fractional change
/// (e.g., 0.0706 = 7.06% change).
pub(crate) fn get_regression_percentage(change_estimates: &Estimates) -> f64 {
    change_estimates.mean.point_estimate * 100.0
}

/// Checks all benchmarks for regressions against a specified limit.
/// Returns a vector of comparison results for all benchmarks.
/// If any benchmark exceeds the regression limit, returns an error with detailed results.
/// Panics if change file is not found for any benchmark.
pub fn check_regressions(
    bench_names: &[&str],
    regression_limit: f64,
) -> Result<Vec<BenchmarkComparison>, (String, Vec<BenchmarkComparison>)> {
    let mut results = Vec::new();
    let mut exceeded_count = 0;

    for bench_name in bench_names {
        let change_estimates = load_change_estimates(bench_name);
        let change_percentage = get_regression_percentage(&change_estimates);
        let exceeds_limit = change_percentage > regression_limit;

        if exceeds_limit {
            exceeded_count += 1;
        }

        results.push(BenchmarkComparison {
            name: bench_name.to_string(),
            change_percentage,
            exceeds_limit,
        });
    }

    if exceeded_count > 0 {
        let error_msg = format!("{} benchmark(s) exceeded regression threshold!", exceeded_count);
        Err((error_msg, results))
    } else {
        Ok(results)
    }
}
