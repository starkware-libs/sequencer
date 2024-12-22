use std::str::FromStr;

use metrics_exporter_prometheus::PrometheusHandle;
use num_traits::Num;
use regex::{escape, Regex};

/// Parses a specific numeric metric value from a Prometheus metrics string.
///
/// # Arguments
///
/// - `prometheus_handle`: A reference to a `PrometheusHandle` that provides access to Prometheus
///   metrics. The metrics are rendered into a string using the `render` method.
/// - `metric`: The name of the metric to search for. The function looks for lines where this metric
///   is followed by whitespace and a numeric value.
///
/// # Type Parameters
///
/// - `T`: The type of the metric value to be parsed. This type must implement the `Num` and
///   `FromStr` traits, ensuring it is a numeric type (e.g., `i32`, `u64`, `f64`) and can be parsed
///   from a string.
///
/// # Returns
///
/// - `Option<T>`: Returns `Some(T)` if the metric is found and successfully parsed into the
///   specified numeric type `T`. Returns `None` if the metric is not found or if parsing fails.
///
/// # Regex Matching
///
/// The function uses a dynamically constructed regular expression to locate the metric and extract
/// its value. The pattern matches lines in the format:
///
/// ```text
/// metric_name <numeric_value>
/// ```
///
/// For example, given the metric name `"http_requests"` and a Prometheus metrics string:
///
/// ```text
/// http_requests 1234
/// memory_usage 5678
/// ```
///
/// The function would extract `1234` as the value for `"http_requests"`.
pub fn parse_numeric_metric<T: Num + FromStr>(
    prometheus_handle: &PrometheusHandle,
    metric: &str,
) -> Option<T> {
    // Render the metrics into a string.
    let output_string: String = prometheus_handle.render();

    // Create a regex to match "metric <number>".
    let pattern = format!(r"{}\s+(\d+)", escape(metric));
    let re = Regex::new(&pattern).expect("Invalid regex");

    // Search for the pattern in the output.
    if let Some(captures) = re.captures(&output_string) {
        // Extract the numeric value.
        if let Some(value) = captures.get(1) {
            // Parse the string into a number.
            return value.as_str().parse().ok();
        }
    }
    // If no match is found, return None.
    None
}
