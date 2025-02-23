use std::str::FromStr;

use metrics::{counter, describe_counter, describe_gauge, gauge, IntoF64};
use num_traits::Num;
use regex::{escape, Regex};

/// Relevant components for which metrics can be defined.
#[derive(Clone, Copy, Debug)]
pub enum MetricScope {
    Batcher,
    HttpServer,
    Infra,
    Network,
    PapyrusSync,
}

pub struct MetricCounter {
    scope: MetricScope,
    name: &'static str,
    description: &'static str,
    initial_value: u64,
}

impl MetricCounter {
    pub const fn new(
        scope: MetricScope,
        name: &'static str,
        description: &'static str,
        initial_value: u64,
    ) -> Self {
        Self { scope, name, description, initial_value }
    }

    pub const fn get_name(&self) -> &'static str {
        self.name
    }

    pub const fn get_scope(&self) -> MetricScope {
        self.scope
    }

    pub const fn get_description(&self) -> &'static str {
        self.description
    }

    pub fn register(&self) {
        counter!(self.name).absolute(self.initial_value);
        describe_counter!(self.name, self.description);
    }

    pub fn increment(&self, value: u64) {
        counter!(self.name).increment(value);
    }

    pub fn parse_numeric_metric<T: Num + FromStr>(&self, metrics_as_string: &str) -> Option<T> {
        parse_numeric_metric::<T>(metrics_as_string, self.get_name(), None)
    }
}

pub struct LabeledMetricCounter {
    scope: MetricScope,
    name: &'static str,
    description: &'static str,
    initial_value: u64,
    registered_labels: Vec<Vec<(&'static str, &'static str)>>,
}

impl LabeledMetricCounter {
    pub const fn new(
        scope: MetricScope,
        name: &'static str,
        description: &'static str,
        initial_value: u64,
    ) -> Self {
        Self { scope, name, description, initial_value, registered_labels: Vec::new() }
    }

    pub const fn get_name(&self) -> &'static str {
        self.name
    }

    pub const fn get_scope(&self) -> MetricScope {
        self.scope
    }

    pub const fn get_description(&self) -> &'static str {
        self.description
    }

    pub fn register(&mut self, label_variations: &[Vec<(&'static str, &'static str)>]) {
        label_variations.iter().for_each(|labels| {
            counter!(self.name, labels).absolute(self.initial_value);
        });
        self.registered_labels = label_variations.into();
        describe_counter!(self.name, self.description);
    }

    pub fn increment(&self, value: u64, labels: &[(&'static str, &'static str)]) {
        assert!(
            self.registered_labels.contains(&labels.to_vec()),
            "Labels {:?} not registered for metric {}",
            labels,
            self.name
        );
        counter!(self.name, labels).increment(value);
    }

    pub fn parse_numeric_metric<T: Num + FromStr>(
        &self,
        metrics_as_string: &str,
        labels: &[(&'static str, &'static str)],
    ) -> Option<T> {
        parse_numeric_metric::<T>(metrics_as_string, self.get_name(), Some(labels))
    }
}

pub struct MetricGauge {
    scope: MetricScope,
    name: &'static str,
    description: &'static str,
}

impl MetricGauge {
    pub const fn new(scope: MetricScope, name: &'static str, description: &'static str) -> Self {
        Self { scope, name, description }
    }

    pub const fn get_name(&self) -> &'static str {
        self.name
    }

    pub const fn get_scope(&self) -> MetricScope {
        self.scope
    }

    pub const fn get_description(&self) -> &'static str {
        self.description
    }

    pub fn register(&self) {
        let _ = gauge!(self.name);
        describe_gauge!(self.name, self.description);
    }

    /// Increments the gauge.
    pub fn increment<T: IntoF64>(&self, value: T) {
        gauge!(self.name).increment(value.into_f64());
    }

    /// Decrements the gauge.
    pub fn decrement<T: IntoF64>(&self, value: T) {
        gauge!(self.name).decrement(value.into_f64());
    }

    pub fn parse_numeric_metric<T: Num + FromStr>(&self, metrics_as_string: &str) -> Option<T> {
        parse_numeric_metric::<T>(metrics_as_string, self.get_name(), None)
    }

    pub fn set<T: IntoF64>(&self, value: T) {
        gauge!(self.name).set(value.into_f64());
    }
}

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
        labels_pattern = format!(r#"\{{{}\}}"#, inner_pattern)
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
