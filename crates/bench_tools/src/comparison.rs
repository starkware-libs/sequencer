use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::PathBuf;

use crate::types::estimates::Estimates;

#[cfg(test)]
#[path = "comparison_test.rs"]
mod comparison_test;

/// Result of a benchmark comparison.
#[derive(Debug)]
pub struct BenchmarkComparison {
    pub name: String,
    /// Point estimate of the change, in percent.
    pub change_percentage: f64,
    /// Bounds of the change confidence interval, in percent.
    pub change_lower_bound_percentage: f64,
    pub change_upper_bound_percentage: f64,
    pub exceeds_regression_limit: bool,
    pub absolute_time_ns: f64,
    pub exceeds_absolute_limit: bool,
}

type RegressionError = (String, Vec<BenchmarkComparison>);
type BenchmarkComparisonsResult = Result<Vec<BenchmarkComparison>, RegressionError>;

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

/// Loads absolute timing estimates from criterion's new directory for a given benchmark.
/// Panics if the estimates file doesn't exist.
fn load_absolute_estimates(bench_name: &str) -> Estimates {
    let estimates_path =
        PathBuf::from("target/criterion").join(bench_name).join("new/estimates.json");

    if !estimates_path.exists() {
        panic!(
            "Estimates file not found for benchmark '{}': {}\nThis likely means the benchmark \
             hasn't been run yet. Run the benchmark before using comparison features.",
            bench_name,
            estimates_path.display()
        );
    }

    let data = fs::read_to_string(&estimates_path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", estimates_path.display(), e));

    serde_json::from_str(&data).unwrap_or_else(|e| {
        panic!("Failed to deserialize {}: {}\nContent: {}", estimates_path.display(), e, data)
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
/// If any benchmark exceeds the regression limit or absolute time threshold, returns an error with
/// detailed results. Panics if change file is not found for any benchmark.
pub fn check_regressions(
    bench_names: &[&str],
    regression_limit: f64,
    absolute_time_ns_limits: &HashMap<String, f64>,
) -> BenchmarkComparisonsResult {
    let loaded_estimates: Vec<(String, Estimates, Estimates)> = bench_names
        .iter()
        .map(|bench_name| {
            (
                bench_name.to_string(),
                load_change_estimates(bench_name),
                load_absolute_estimates(bench_name),
            )
        })
        .collect();

    compare_estimates(&loaded_estimates, regression_limit, absolute_time_ns_limits)
}

/// Decides, for each benchmark, whether its change and absolute time are within the limits.
///
/// A benchmark counts as a regression only when the LOWER bound of the change confidence interval
/// exceeds `regression_limit`, meaning the measurement is confident the regression is real and
/// larger than the limit. Gating on the point estimate alone lets run-to-run noise fail the job:
/// the same commit measured twice has produced +8.23% and +31.59% on the same benchmark.
pub fn compare_estimates(
    loaded_estimates: &[(String, Estimates, Estimates)],
    regression_limit: f64,
    absolute_time_ns_limits: &HashMap<String, f64>,
) -> BenchmarkComparisonsResult {
    // An absolute limit naming a benchmark that is not being compared silently guards nothing, so
    // a typo disables the limit without any signal. Fail loudly instead.
    let compared_benchmark_names: BTreeSet<&str> =
        loaded_estimates.iter().map(|(bench_name, _, _)| bench_name.as_str()).collect();
    let unmatched_limit_names: Vec<&str> = absolute_time_ns_limits
        .keys()
        .map(String::as_str)
        .filter(|limit_name| !compared_benchmark_names.contains(limit_name))
        .collect();
    assert!(
        unmatched_limit_names.is_empty(),
        "Absolute time limits name benchmarks that are not being compared: \
         {unmatched_limit_names:?}. Benchmarks being compared: {compared_benchmark_names:?}."
    );

    let mut results = Vec::new();
    let mut exceeded_count = 0;

    for (bench_name, change_estimates, absolute_estimates) in loaded_estimates {
        let change_percentage = get_regression_percentage(change_estimates);
        let change_lower_bound_percentage =
            change_estimates.mean.confidence_interval.lower_bound * 100.0;
        let change_upper_bound_percentage =
            change_estimates.mean.confidence_interval.upper_bound * 100.0;
        let exceeds_regression_limit = change_lower_bound_percentage > regression_limit;

        let absolute_time_ns = absolute_estimates.mean.point_estimate;

        // Check if this benchmark has a specific absolute time limit.
        let exceeds_absolute_limit =
            if let Some(&threshold) = absolute_time_ns_limits.get(bench_name) {
                absolute_time_ns > threshold
            } else {
                false
            };

        if exceeds_regression_limit || exceeds_absolute_limit {
            exceeded_count += 1;
        }

        results.push(BenchmarkComparison {
            name: bench_name.to_string(),
            change_percentage,
            change_lower_bound_percentage,
            change_upper_bound_percentage,
            exceeds_regression_limit,
            absolute_time_ns,
            exceeds_absolute_limit,
        });
    }

    if exceeded_count > 0 {
        let error_msg = format!("{} benchmark(s) exceeded threshold(s)!", exceeded_count);
        Err((error_msg, results))
    } else {
        Ok(results)
    }
}
