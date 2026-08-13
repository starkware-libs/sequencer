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
    /// Point estimate of the mean time, in nanoseconds.
    pub absolute_time_ns: f64,
    /// Lower bound of the mean time confidence interval, in nanoseconds.
    pub absolute_time_lower_bound_ns: f64,
    pub exceeds_absolute_limit: bool,
    pub has_non_finite_estimate: bool,
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

/// Returns the names in `absolute_time_ns_limits` that are not in `bench_names`.
pub fn find_unknown_limit_names<'a>(
    bench_names: &[&str],
    absolute_time_ns_limits: &'a HashMap<String, f64>,
) -> BTreeSet<&'a str> {
    absolute_time_ns_limits
        .keys()
        .map(String::as_str)
        .filter(|limit_name| !bench_names.contains(limit_name))
        .collect()
}

/// Decides, for each benchmark, whether its change and absolute time are within the limits.
///
/// Both limits apply to the lower bound of a confidence interval: a benchmark fails when its change
/// lower bound exceeds `regression_limit`, or when its mean time lower bound exceeds its entry in
/// `absolute_time_ns_limits`. A non-finite estimate also fails the benchmark, and a limit naming a
/// benchmark that is not in `loaded_estimates` fails the comparison.
pub fn compare_estimates(
    loaded_estimates: &[(String, Estimates, Estimates)],
    regression_limit: f64,
    absolute_time_ns_limits: &HashMap<String, f64>,
) -> BenchmarkComparisonsResult {
    let mut results = Vec::new();
    let mut exceeded_count = 0;
    let mut non_finite_count = 0;

    for (bench_name, change_estimates, absolute_estimates) in loaded_estimates {
        let change_percentage = get_regression_percentage(change_estimates);
        let change_lower_bound_percentage =
            change_estimates.mean.confidence_interval.lower_bound * 100.0;
        let change_upper_bound_percentage =
            change_estimates.mean.confidence_interval.upper_bound * 100.0;
        let exceeds_regression_limit = change_lower_bound_percentage > regression_limit;

        let absolute_time_ns = absolute_estimates.mean.point_estimate;
        let absolute_time_lower_bound_ns = absolute_estimates.mean.confidence_interval.lower_bound;

        // Check if this benchmark has a specific absolute time limit.
        let exceeds_absolute_limit =
            if let Some(&threshold) = absolute_time_ns_limits.get(bench_name) {
                absolute_time_lower_bound_ns > threshold
            } else {
                false
            };

        let has_non_finite_estimate = [
            change_percentage,
            change_lower_bound_percentage,
            change_upper_bound_percentage,
            absolute_time_ns,
            absolute_time_lower_bound_ns,
        ]
        .iter()
        .any(|estimate| !estimate.is_finite());

        if exceeds_regression_limit || exceeds_absolute_limit {
            exceeded_count += 1;
        }
        if has_non_finite_estimate {
            non_finite_count += 1;
        }

        results.push(BenchmarkComparison {
            name: bench_name.to_string(),
            change_percentage,
            change_lower_bound_percentage,
            change_upper_bound_percentage,
            exceeds_regression_limit,
            absolute_time_ns,
            absolute_time_lower_bound_ns,
            exceeds_absolute_limit,
            has_non_finite_estimate,
        });
    }

    let mut error_messages = Vec::new();
    if exceeded_count > 0 {
        error_messages.push(format!("{exceeded_count} benchmark(s) exceeded threshold(s)!"));
    }
    if non_finite_count > 0 {
        error_messages.push(format!("{non_finite_count} benchmark(s) have non-finite estimates!"));
    }
    let compared_benchmark_names: Vec<&str> =
        loaded_estimates.iter().map(|(bench_name, _, _)| bench_name.as_str()).collect();
    let unknown_limit_names =
        find_unknown_limit_names(&compared_benchmark_names, absolute_time_ns_limits);
    if !unknown_limit_names.is_empty() {
        error_messages.push(format!(
            "Absolute time limits name benchmarks that are not being compared: \
             {unknown_limit_names:?}. Benchmarks being compared: {compared_benchmark_names:?}."
        ));
    }

    if error_messages.is_empty() { Ok(results) } else { Err((error_messages.join("\n"), results)) }
}
