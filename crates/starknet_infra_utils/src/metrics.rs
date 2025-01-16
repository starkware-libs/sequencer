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
) -> Option<T> {
    // Create a regex to match "metric_name <number>".
    let pattern = format!(r"{}\s+(\d+)", escape(metric_name));
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
