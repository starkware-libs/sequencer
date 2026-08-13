use std::collections::{BTreeSet, HashMap};

use rstest::rstest;

use crate::comparison::{compare_estimates, find_unknown_limit_names};
use crate::types::estimates::{ConfidenceInterval, Estimates, Stat};

const VM_BENCHMARK_NAME: &str = "transfers_sequential_benchmark_vm";
const CAIRO_NATIVE_BENCHMARK_NAME: &str = "transfers_sequential_benchmark_cairo_native";
// Test-only limits, independent of the limits the CI workflows pass to `bench_tools`.
const REGRESSION_LIMIT: f64 = 8.0;
const VM_TIME_LIMIT_NS: f64 = 1_600_000_000.0;
const CAIRO_NATIVE_TIME_LIMIT_NS: f64 = 300_000_000.0;
const VM_TIME_WITHIN_LIMIT_NS: f64 = VM_TIME_LIMIT_NS / 2.0;
const CAIRO_NATIVE_TIME_WITHIN_LIMIT_NS: f64 = CAIRO_NATIVE_TIME_LIMIT_NS / 2.0;

/// Builds change estimates from a percentage, matching criterion's fractional representation
/// (0.0706 means +7.06%). The confidence interval spans 10 percentage points on each side of the
/// point estimate, as in a noisy run.
fn change_estimates(point_estimate_percentage: f64) -> Estimates {
    Estimates {
        mean: Stat {
            point_estimate: point_estimate_percentage / 100.0,
            standard_error: 0.0,
            confidence_interval: ConfidenceInterval {
                confidence_level: 0.95,
                lower_bound: (point_estimate_percentage - 10.0) / 100.0,
                upper_bound: (point_estimate_percentage + 10.0) / 100.0,
            },
        },
        ..Default::default()
    }
}

/// Builds mean time estimates whose confidence interval spans 10% on each side of the point
/// estimate.
fn absolute_estimates(point_estimate_ns: f64) -> Estimates {
    Estimates {
        mean: Stat {
            point_estimate: point_estimate_ns,
            standard_error: 0.0,
            confidence_interval: ConfidenceInterval {
                confidence_level: 0.95,
                lower_bound: point_estimate_ns * 0.9,
                upper_bound: point_estimate_ns * 1.1,
            },
        },
        ..Default::default()
    }
}

fn vm_limits() -> HashMap<String, f64> {
    HashMap::from([(VM_BENCHMARK_NAME.to_string(), VM_TIME_LIMIT_NS)])
}

/// The comparison is strict: a change equal to the limit passes.
#[rstest]
#[case::below_the_limit(REGRESSION_LIMIT - 0.01, false)]
#[case::equal_to_the_limit(REGRESSION_LIMIT, false)]
#[case::above_the_limit(REGRESSION_LIMIT + 0.01, true)]
#[case::improvement(-12.00, false)]
fn change_point_estimate_is_compared_to_the_regression_limit(
    #[case] change_percentage: f64,
    #[case] expected_exceeds_regression_limit: bool,
) {
    let loaded_estimates = vec![(
        VM_BENCHMARK_NAME.to_string(),
        change_estimates(change_percentage),
        absolute_estimates(VM_TIME_WITHIN_LIMIT_NS),
    )];

    let result = compare_estimates(&loaded_estimates, REGRESSION_LIMIT, &vm_limits());

    let (Ok(results) | Err((_, results))) = &result;
    assert_eq!(results.len(), 1);
    assert!((results[0].change_percentage - change_percentage).abs() < 1e-9);
    assert_eq!(results[0].exceeds_regression_limit, expected_exceeds_regression_limit);
    assert!(!results[0].exceeds_absolute_limit);
    assert!(!results[0].has_non_finite_estimate);
    match result {
        Ok(_) => assert!(!expected_exceeds_regression_limit),
        Err((error_message, _)) => {
            assert!(expected_exceeds_regression_limit);
            assert_eq!(error_message, "1 benchmark(s) exceeded threshold(s)!");
        }
    }
}

/// The comparison is strict: a mean time equal to the limit passes.
#[rstest]
#[case::below_the_limit(VM_TIME_LIMIT_NS - 1.0, false)]
#[case::equal_to_the_limit(VM_TIME_LIMIT_NS, false)]
#[case::above_the_limit(VM_TIME_LIMIT_NS + 1.0, true)]
fn mean_time_point_estimate_is_compared_to_the_absolute_limit(
    #[case] absolute_time_ns: f64,
    #[case] expected_exceeds_absolute_limit: bool,
) {
    let loaded_estimates = vec![(
        VM_BENCHMARK_NAME.to_string(),
        change_estimates(1.00),
        absolute_estimates(absolute_time_ns),
    )];

    let result = compare_estimates(&loaded_estimates, REGRESSION_LIMIT, &vm_limits());

    let (Ok(results) | Err((_, results))) = &result;
    assert_eq!(results[0].absolute_time_ns, absolute_time_ns);
    assert_eq!(results[0].exceeds_absolute_limit, expected_exceeds_absolute_limit);
    assert!(!results[0].exceeds_regression_limit);
    assert_eq!(result.is_err(), expected_exceeds_absolute_limit);
}

/// Each benchmark is checked only against its own absolute limit, and every benchmark that exceeds
/// a limit is counted once in the error.
#[test]
fn each_benchmark_is_checked_against_its_own_limits() {
    let loaded_estimates = vec![
        // Over the relative limit only.
        (
            VM_BENCHMARK_NAME.to_string(),
            change_estimates(REGRESSION_LIMIT + 1.0),
            absolute_estimates(VM_TIME_WITHIN_LIMIT_NS),
        ),
        // Over its own absolute limit only.
        (
            CAIRO_NATIVE_BENCHMARK_NAME.to_string(),
            change_estimates(1.00),
            absolute_estimates(CAIRO_NATIVE_TIME_LIMIT_NS + 1.0),
        ),
        // No absolute limit, so its mean time is not checked.
        (
            "unlimited_benchmark".to_string(),
            change_estimates(1.00),
            absolute_estimates(10.0 * VM_TIME_LIMIT_NS),
        ),
    ];
    let absolute_time_ns_limits = HashMap::from([
        (VM_BENCHMARK_NAME.to_string(), VM_TIME_LIMIT_NS),
        (CAIRO_NATIVE_BENCHMARK_NAME.to_string(), CAIRO_NATIVE_TIME_LIMIT_NS),
    ]);

    let (error_message, results) =
        compare_estimates(&loaded_estimates, REGRESSION_LIMIT, &absolute_time_ns_limits)
            .unwrap_err();

    assert_eq!(error_message, "2 benchmark(s) exceeded threshold(s)!");
    let verdicts: Vec<(&str, bool, bool)> = results
        .iter()
        .map(|result| {
            (result.name.as_str(), result.exceeds_regression_limit, result.exceeds_absolute_limit)
        })
        .collect();
    assert_eq!(
        verdicts,
        vec![
            (VM_BENCHMARK_NAME, true, false),
            (CAIRO_NATIVE_BENCHMARK_NAME, false, true),
            ("unlimited_benchmark", false, false),
        ]
    );
}

#[test]
fn benchmarks_within_all_limits_pass() {
    let loaded_estimates = vec![
        (
            VM_BENCHMARK_NAME.to_string(),
            change_estimates(-2.93),
            absolute_estimates(VM_TIME_WITHIN_LIMIT_NS),
        ),
        (
            CAIRO_NATIVE_BENCHMARK_NAME.to_string(),
            change_estimates(4.10),
            absolute_estimates(CAIRO_NATIVE_TIME_WITHIN_LIMIT_NS),
        ),
    ];
    let absolute_time_ns_limits = HashMap::from([
        (VM_BENCHMARK_NAME.to_string(), VM_TIME_LIMIT_NS),
        (CAIRO_NATIVE_BENCHMARK_NAME.to_string(), CAIRO_NATIVE_TIME_LIMIT_NS),
    ]);

    let results =
        compare_estimates(&loaded_estimates, REGRESSION_LIMIT, &absolute_time_ns_limits).unwrap();

    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|result| !result.exceeds_regression_limit
        && !result.exceeds_absolute_limit
        && !result.has_non_finite_estimate));
}

#[test]
fn no_benchmarks_produces_no_comparisons() {
    let results = compare_estimates(&[], REGRESSION_LIMIT, &HashMap::new()).unwrap();

    assert!(results.is_empty());
}

/// A NaN point estimate compares false against the limit, so only the non-finite check fails it.
#[rstest]
#[case::nan_change(f64::NAN, VM_TIME_WITHIN_LIMIT_NS)]
#[case::infinite_change(f64::INFINITY, VM_TIME_WITHIN_LIMIT_NS)]
#[case::nan_mean_time(1.00, f64::NAN)]
#[case::infinite_mean_time(1.00, f64::INFINITY)]
fn non_finite_point_estimate_fails_the_comparison(
    #[case] change_percentage: f64,
    #[case] absolute_time_ns: f64,
) {
    let loaded_estimates = vec![(
        VM_BENCHMARK_NAME.to_string(),
        change_estimates(change_percentage),
        absolute_estimates(absolute_time_ns),
    )];

    let (error_message, results) =
        compare_estimates(&loaded_estimates, REGRESSION_LIMIT, &vm_limits()).unwrap_err();

    assert!(results[0].has_non_finite_estimate);
    assert!(error_message.contains("1 benchmark(s) have non-finite estimates!"));
}

#[test]
fn absolute_limit_naming_an_unknown_benchmark_is_an_error_with_results() {
    let loaded_estimates = vec![(
        VM_BENCHMARK_NAME.to_string(),
        change_estimates(1.00),
        absolute_estimates(VM_TIME_WITHIN_LIMIT_NS),
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
        (VM_BENCHMARK_NAME.to_string(), VM_TIME_LIMIT_NS),
        ("transfers_benchmark_vm".to_string(), 1_000_000_000.0),
    ]);

    let unknown_limit_names = find_unknown_limit_names(
        &[VM_BENCHMARK_NAME, CAIRO_NATIVE_BENCHMARK_NAME],
        &absolute_time_ns_limits,
    );

    assert_eq!(unknown_limit_names, BTreeSet::from(["transfers_benchmark_vm"]));
}
