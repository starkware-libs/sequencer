use std::collections::HashMap;

use super::compare_estimates;
use crate::types::estimates::{ConfidenceInterval, Estimates, Stat};

const REGRESSION_LIMIT: f64 = 40.0;

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

/// A measurement too noisy to conclude anything from: the point estimate is above the limit, but
/// the interval straddles it, so the run does not support calling this a regression. The point
/// estimate is deliberately above `REGRESSION_LIMIT`, so gating on it would fail here and only the
/// lower-bound check lets it pass.
///
/// The observed +31.59% on `transfers_sequential_benchmark_vm` had a much tighter interval and is
/// held back by the limit rather than by the interval; this covers the complementary case.
#[test]
fn noisy_change_with_interval_below_the_limit_is_not_a_regression() {
    let loaded_estimates = vec![(
        "transfers_sequential_benchmark_vm".to_string(),
        change_estimates(45.00, 12.00, 78.00),
        absolute_estimates(500_000_000.0),
    )];

    let results = compare_estimates(&loaded_estimates, REGRESSION_LIMIT, &HashMap::new()).unwrap();

    assert_eq!(results.len(), 1);
    assert!(!results[0].exceeds_regression_limit);
    assert_eq!(results[0].change_percentage, 45.00);
}

/// A real regression: the whole confidence interval sits above the limit, so even the most
/// favourable reading of the measurement is a regression larger than the limit.
#[test]
fn change_with_interval_above_the_limit_is_a_regression() {
    let loaded_estimates = vec![(
        "transfers_sequential_benchmark_vm".to_string(),
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
        "transfers_sequential_benchmark_cairo_native".to_string(),
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
        "transfers_sequential_benchmark_cairo_native".to_string(),
        change_estimates(1.00, -3.00, 5.00),
        absolute_estimates(250_000_000.0),
    )];
    let absolute_time_ns_limits =
        HashMap::from([("transfers_sequential_benchmark_cairo_native".to_string(), 200_000_000.0)]);

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

/// An absolute limit naming a benchmark that is not being compared guards nothing. That is how the
/// `transfers_benchmark_*` limits sat inert while the real benchmarks are named
/// `transfers_sequential_benchmark_*`, so the mismatch has to be loud.
#[test]
#[should_panic(expected = "name benchmarks that are not being compared")]
fn absolute_limit_naming_an_unknown_benchmark_panics() {
    let loaded_estimates = vec![(
        "transfers_sequential_benchmark_vm".to_string(),
        change_estimates(1.00, -3.00, 5.00),
        absolute_estimates(843_240_000.0),
    )];
    let absolute_time_ns_limits =
        HashMap::from([("transfers_benchmark_vm".to_string(), 1_000_000_000.0)]);

    let _ = compare_estimates(&loaded_estimates, REGRESSION_LIMIT, &absolute_time_ns_limits);
}
