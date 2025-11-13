use std::fmt::Debug;
use std::str::FromStr;

use num_traits::Num;
use regex::{escape, Regex};

/// Parses a specific numeric metric value from a metrics string.
///
/// # Arguments
///
/// - `metrics_as_string`: A string containing the renders metrics data.
/// - `metric_name`: The name of the metric to search for.
///
/// # Type Parameters
///
/// - `T`: The numeric type to which the metric value will be parsed. The type must implement the
///   `Num` and `FromStr` traits, allowing it to represent numeric values and be parsed from a
///   string. Common types include `i32`, `u64`, and `f64`.
///
/// # Returns
///
/// - `Option<T>`: Returns `Some(T)` if the metric is found and successfully parsed into the
///   specified numeric type `T`. Returns `None` if the metric is not found or if parsing fails.
pub fn parse_numeric_metric<T: Num + FromStr>(
    metrics_as_string: &str,
    metric_name: &str,
    labels: Option<&[(&'static str, &'static str)]>,
) -> Option<T> {
    // Construct a regex pattern to match Prometheus-style metrics.
    // - If there are no labels, it matches: "metric_name <number>" (e.g., `http_requests_total
    //   123`).
    // - If labels are present, it matches: "metric_name{label1="value1",label2="value2",...}
    //   <number>" (e.g., `http_requests_total{method="POST",status="200"} 123`).
    let mut labels_pattern = "".to_string();
    if let Some(labels) = labels {
        // Create a regex to match "{label1="value1",label2="value2",...}".
        let inner_pattern = labels
            .iter()
            .map(|(k, v)| format!(r#"{}="{}""#, escape(k), escape(v)))
            .collect::<Vec<_>>()
            .join(r",");
        labels_pattern = format!(r#"\{{{inner_pattern}\}}"#)
    };
    let pattern = format!(r#"{}{}\s+(\d+)"#, escape(metric_name), labels_pattern);
    let re = Regex::new(&pattern).expect("Invalid regex");

    // Search for the pattern in the output.
    if let Some(captures) = re.captures(metrics_as_string) {
        // Extract the numeric value.
        if let Some(value) = captures.get(1) {
            // Parse the string into a number.
            return value.as_str().parse().ok();
        }
    }
    // If no match is found, return None.
    None
}

pub(crate) fn assert_equality<T: PartialEq + Debug>(
    value: &T,
    expected_value: &T,
    metric_name: &str,
    label: Option<&[(&str, &str)]>,
) {
    let label_msg = label.map(|l| format!(" {l:?}")).unwrap_or_default();
    assert_eq!(
        value, expected_value,
        "Metric {metric_name}{label_msg} did not match the expected value. Expected value: \
         {expected_value:?}, metric value: {value:?}"
    );
}

pub(crate) fn assert_metric_exists(metrics_as_string: &str, metric_name: &str, metric_type: &str) {
    let expected_string = format!("# TYPE {metric_name} {metric_type}\n{metric_name}");
    assert!(
        metrics_as_string.contains(&expected_string),
        "Metric {metric_name} of type {metric_type} does not exist in the provided metrics \
         string:\n{metrics_as_string}"
    );
}
