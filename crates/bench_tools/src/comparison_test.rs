use std::collections::HashMap;

use super::compare_estimates;
use crate::types::estimates::{ConfidenceInterval, Estimates, Stat};

const REGRESSION_LIMIT: f64 = 35.0;

/// Builds change estimates from percentages, matching criterion's fractional representation
/// (0.0706 means +7.06%).
fn change_estimates(
    point_estimate_percentage: f64,
    lower_bound_percentage: f64,
    upper_bound_percentage: f64,
) -> Estimates {
    Estimates {
        mean: Stat {
            point_estimate: point_estimate_percentage / 100.0,
            standard_error: 0.0,
            confidence_interval: ConfidenceInterval {
                confidence_level: 0.95,
                lower_bound: lower_bound_percentage / 100.0,
                upper_bound: upper_bound_percentage / 100.0,
            },
        },
        ..Default::default()
    }
}

fn absolute_estimates(time_ns: f64) -> Estimates {
    Estimates { mean: Stat { point_estimate: time_ns, ..Default::default() }, ..Default::default() }
}

/// The false regression this gate exists to reject: the same commit measured twice reported
/// +31.59% on `transfers_sequential_benchmark_vm`. A noisy measurement like that has a confidence
/// interval wide enough to straddle the limit, so it must not fail the job.
#[test]
fn noisy_change_with_interval_below_the_limit_is_not_a_regression() {
    let loaded_estimates = vec![(
        "transfers_sequential_benchmark_vm".to_string(),
        change_estimates(31.59, 2.10, 64.30),
        absolute_estimates(500_000_000.0),
    )];

    let results = compare_estimates(&loaded_estimates, REGRESSION_LIMIT, &HashMap::new()).unwrap();

    assert_eq!(results.len(), 1);
    assert!(!results[0].exceeds_regression_limit);
    assert_eq!(results[0].change_percentage, 31.59);
}

/// A real regression: the whole confidence interval sits above the limit, so even the most
/// favourable reading of the measurement is a regression larger than the limit.
#[test]
fn change_with_interval_above_the_limit_is_a_regression() {
    let loaded_estimates = vec![(
        "transfers_benchmark_vm".to_string(),
        change_estimates(52.00, 48.10, 55.90),
        absolute_estimates(500_000_000.0),
    )];

    let (error_message, results) =
        compare_estimates(&loaded_estimates, REGRESSION_LIMIT, &HashMap::new()).unwrap_err();

    assert!(results[0].exceeds_regression_limit);
    assert_eq!(results[0].change_lower_bound_percentage, 48.10);
    assert!(error_message.contains("1 benchmark(s) exceeded"));
}

#[test]
fn change_below_the_limit_passes() {
    let loaded_estimates = vec![(
        "transfers_benchmark_cairo_native".to_string(),
        change_estimates(-2.93, -6.40, 0.55),
        absolute_estimates(196_992_050.0),
    )];

    let results = compare_estimates(&loaded_estimates, REGRESSION_LIMIT, &HashMap::new()).unwrap();

    assert!(!results[0].exceeds_regression_limit);
    assert!(!results[0].exceeds_absolute_limit);
}

/// The absolute time limit is an independent backstop: a benchmark that is slow in absolute terms
/// fails even when its change is well inside the relative limit.
#[test]
fn absolute_limit_trips_independently_of_the_relative_check() {
    let loaded_estimates = vec![(
        "transfers_benchmark_cairo_native".to_string(),
        change_estimates(1.00, -3.00, 5.00),
        absolute_estimates(250_000_000.0),
    )];
    let absolute_time_ns_limits =
        HashMap::from([("transfers_benchmark_cairo_native".to_string(), 200_000_000.0)]);

    let (_error_message, results) =
        compare_estimates(&loaded_estimates, REGRESSION_LIMIT, &absolute_time_ns_limits)
            .unwrap_err();

    assert!(results[0].exceeds_absolute_limit);
    assert!(!results[0].exceeds_regression_limit);
}

#[test]
fn no_benchmarks_produces_no_comparisons() {
    let results = compare_estimates(&[], REGRESSION_LIMIT, &HashMap::new()).unwrap();

    assert!(results.is_empty());
}
