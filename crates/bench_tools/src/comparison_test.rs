use std::collections::{BTreeSet, HashMap};

use rstest::rstest;

use crate::comparison::{compare_estimates, find_unknown_limit_names};
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

fn absolute_estimates(point_estimate_ns: f64, lower_bound_ns: f64) -> Estimates {
    Estimates {
        mean: Stat {
            point_estimate: point_estimate_ns,
            confidence_interval: ConfidenceInterval {
                lower_bound: lower_bound_ns,
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    }
}

/// A measurement too noisy to conclude anything from: the point estimate is above the limit, but
/// the interval straddles it, so the run does not support calling this a regression. The point
/// estimate is deliberately above `REGRESSION_LIMIT`, so gating on it would fail here and only the
/// lower-bound check lets it pass.
#[test]
fn noisy_change_with_interval_straddling_the_limit_is_not_a_regression() {
    let loaded_estimates = vec![(
        "transfers_sequential_benchmark_vm".to_string(),
        change_estimates(45.00, 12.00, 78.00),
        absolute_estimates(500_000_000.0, 490_000_000.0),
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
        absolute_estimates(500_000_000.0, 490_000_000.0),
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
        absolute_estimates(196_992_050.0, 194_460_000.0),
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
        absolute_estimates(250_000_000.0, 240_000_000.0),
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

#[test]
fn absolute_limit_naming_an_unknown_benchmark_is_an_error_with_results() {
    let loaded_estimates = vec![(
        "transfers_sequential_benchmark_vm".to_string(),
        change_estimates(1.00, -3.00, 5.00),
        absolute_estimates(843_240_000.0, 830_000_000.0),
    )];
    let absolute_time_ns_limits =
        HashMap::from([("transfers_benchmark_vm".to_string(), 1_000_000_000.0)]);

    let (error_message, results) =
        compare_estimates(&loaded_estimates, REGRESSION_LIMIT, &absolute_time_ns_limits)
            .unwrap_err();

    assert!(error_message.contains("name benchmarks that are not being compared"));
    assert!(error_message.contains("transfers_benchmark_vm"));
    assert_eq!(results.len(), 1);
    assert!(!results[0].exceeds_regression_limit);
    assert!(!results[0].exceeds_absolute_limit);
}

#[test]
fn find_unknown_limit_names_returns_only_unmatched_names() {
    let absolute_time_ns_limits = HashMap::from([
        ("transfers_sequential_benchmark_vm".to_string(), 1_600_000_000.0),
        ("transfers_benchmark_vm".to_string(), 1_000_000_000.0),
    ]);

    let unknown_limit_names = find_unknown_limit_names(
        &["transfers_sequential_benchmark_vm", "transfers_sequential_benchmark_cairo_native"],
        &absolute_time_ns_limits,
    );

    assert_eq!(unknown_limit_names, BTreeSet::from(["transfers_benchmark_vm"]));
}

/// The comparison is strict: a lower bound equal to the limit passes.
#[test]
fn change_lower_bound_equal_to_the_limit_is_not_a_regression() {
    let loaded_estimates = vec![(
        "transfers_sequential_benchmark_vm".to_string(),
        change_estimates(45.00, REGRESSION_LIMIT, 50.00),
        absolute_estimates(500_000_000.0, 490_000_000.0),
    )];

    let results = compare_estimates(&loaded_estimates, REGRESSION_LIMIT, &HashMap::new()).unwrap();

    assert!(!results[0].exceeds_regression_limit);
}

#[test]
fn absolute_limit_applies_to_the_mean_time_lower_bound() {
    let loaded_estimates = vec![(
        "transfers_sequential_benchmark_vm".to_string(),
        change_estimates(1.00, -3.00, 5.00),
        absolute_estimates(1_700_000_000.0, 1_500_000_000.0),
    )];
    let absolute_time_ns_limits =
        HashMap::from([("transfers_sequential_benchmark_vm".to_string(), 1_600_000_000.0)]);

    let results =
        compare_estimates(&loaded_estimates, REGRESSION_LIMIT, &absolute_time_ns_limits).unwrap();

    assert!(!results[0].exceeds_absolute_limit);
}

#[test]
fn absolute_lower_bound_equal_to_the_limit_is_within_the_limit() {
    let loaded_estimates = vec![(
        "transfers_sequential_benchmark_vm".to_string(),
        change_estimates(1.00, -3.00, 5.00),
        absolute_estimates(1_650_000_000.0, 1_600_000_000.0),
    )];
    let absolute_time_ns_limits =
        HashMap::from([("transfers_sequential_benchmark_vm".to_string(), 1_600_000_000.0)]);

    let results =
        compare_estimates(&loaded_estimates, REGRESSION_LIMIT, &absolute_time_ns_limits).unwrap();

    assert!(!results[0].exceeds_absolute_limit);
}

#[rstest]
#[case::nan_change_lower_bound(
    change_estimates(1.00, f64::NAN, 5.00),
    absolute_estimates(500_000_000.0, 490_000_000.0)
)]
#[case::infinite_change_point_estimate(
    change_estimates(f64::INFINITY, -3.00, 5.00),
    absolute_estimates(500_000_000.0, 490_000_000.0)
)]
#[case::nan_absolute_lower_bound(
    change_estimates(1.00, -3.00, 5.00),
    absolute_estimates(500_000_000.0, f64::NAN)
)]
#[case::infinite_absolute_point_estimate(
    change_estimates(1.00, -3.00, 5.00),
    absolute_estimates(f64::INFINITY, 490_000_000.0)
)]
fn non_finite_estimate_fails_the_comparison(
    #[case] change_estimates: Estimates,
    #[case] absolute_estimates: Estimates,
) {
    let loaded_estimates = vec![(
        "transfers_sequential_benchmark_vm".to_string(),
        change_estimates,
        absolute_estimates,
    )];
    let absolute_time_ns_limits =
        HashMap::from([("transfers_sequential_benchmark_vm".to_string(), 1_600_000_000.0)]);

    let (error_message, results) =
        compare_estimates(&loaded_estimates, REGRESSION_LIMIT, &absolute_time_ns_limits)
            .unwrap_err();

    assert!(results[0].has_non_finite_estimate);
    assert!(error_message.contains("1 benchmark(s) have non-finite estimates"));
}
