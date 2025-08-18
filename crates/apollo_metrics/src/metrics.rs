use std::fmt::Debug;
#[cfg(any(feature = "testing", test))]
use std::str::FromStr;
use std::sync::OnceLock;

use indexmap::IndexMap;
use metrics::{
    counter,
    describe_counter,
    describe_gauge,
    describe_histogram,
    gauge,
    histogram,
    IntoF64,
};
#[cfg(any(feature = "testing", test))]
use num_traits::Num;
#[cfg(any(feature = "testing", test))]
use regex::{escape, Regex};

use crate::metric_label_filter;

#[cfg(test)]
#[path = "metrics_test.rs"]
mod metrics_tests;

/// Global variable set by the main config to enable collecting profiling metrics.
pub static COLLECT_SEQUENCER_PROFILING_METRICS: OnceLock<bool> = OnceLock::new();

/// Relevant components for which metrics can be defined.
#[derive(Clone, Copy, Debug)]
pub enum MetricScope {
    Batcher,
    Blockifier,
    ClassManager,
    Consensus,
    ConsensusManager,
    ConsensusOrchestrator,
    Gateway,
    HttpServer,
    Infra,
    L1GasPrice,
    L1Provider,
    Mempool,
    MempoolP2p,
    CompileToCasm,
    StateSync,
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

    pub fn get_name_with_filter(&self) -> String {
        format!("{}{}", self.name, metric_label_filter!())
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

    #[cfg(any(feature = "testing", test))]
    pub fn parse_numeric_metric<T: Num + FromStr>(&self, metrics_as_string: &str) -> Option<T> {
        parse_numeric_metric::<T>(metrics_as_string, self.get_name(), None)
    }

    #[cfg(any(feature = "testing", test))]
    #[track_caller]
    pub fn assert_eq<T: Num + FromStr + Debug>(&self, metrics_as_string: &str, expected_value: T) {
        let metric_value = self.parse_numeric_metric::<T>(metrics_as_string).unwrap();
        assert_equality(&metric_value, &expected_value, self.get_name(), None);
    }
}

pub struct LabeledMetricCounter {
    scope: MetricScope,
    name: &'static str,
    description: &'static str,
    initial_value: u64,
    label_permutations: &'static [&'static [(&'static str, &'static str)]],
}

impl LabeledMetricCounter {
    pub const fn new(
        scope: MetricScope,
        name: &'static str,
        description: &'static str,
        initial_value: u64,
        label_permutations: &'static [&'static [(&'static str, &'static str)]],
    ) -> Self {
        Self { scope, name, description, initial_value, label_permutations }
    }

    pub const fn get_name(&self) -> &'static str {
        self.name
    }

    pub fn get_name_with_filter(&self) -> String {
        format!("{}{}", self.name, metric_label_filter!())
    }

    pub const fn get_scope(&self) -> MetricScope {
        self.scope
    }

    pub const fn get_description(&self) -> &'static str {
        self.description
    }

    pub fn register(&self) {
        self.label_permutations.iter().map(|&slice| slice.to_vec()).for_each(|labels| {
            counter!(self.name, &labels).absolute(self.initial_value);
        });
        describe_counter!(self.name, self.description);
    }

    pub fn increment(&self, value: u64, labels: &[(&'static str, &'static str)]) {
        counter!(self.name, labels).increment(value);
    }

    #[cfg(any(feature = "testing", test))]
    pub fn parse_numeric_metric<T: Num + FromStr>(
        &self,
        metrics_as_string: &str,
        labels: &[(&'static str, &'static str)],
    ) -> Option<T> {
        parse_numeric_metric::<T>(metrics_as_string, self.get_name(), Some(labels))
    }

    #[cfg(any(feature = "testing", test))]
    #[track_caller]
    pub fn assert_eq<T: Num + FromStr + Debug>(
        &self,
        metrics_as_string: &str,
        expected_value: T,
        label: &[(&'static str, &'static str)],
    ) {
        let metric_value = self.parse_numeric_metric::<T>(metrics_as_string, label).unwrap();
        assert_equality(&metric_value, &expected_value, self.get_name(), Some(label));
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

    pub fn get_name_with_filter(&self) -> String {
        format!("{}{}", self.name, metric_label_filter!())
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

    pub fn increment<T: IntoF64>(&self, value: T) {
        gauge!(self.name).increment(value.into_f64());
    }

    pub fn decrement<T: IntoF64>(&self, value: T) {
        gauge!(self.name).decrement(value.into_f64());
    }

    #[cfg(any(feature = "testing", test))]
    pub fn parse_numeric_metric<T: Num + FromStr>(&self, metrics_as_string: &str) -> Option<T> {
        parse_numeric_metric::<T>(metrics_as_string, self.get_name(), None)
    }

    pub fn set<T: IntoF64>(&self, value: T) {
        gauge!(self.name).set(value.into_f64());
    }

    pub fn set_lossy<T: LossyIntoF64>(&self, value: T) {
        gauge!(self.name).set(value.into_f64());
    }

    #[cfg(any(feature = "testing", test))]
    #[track_caller]
    pub fn assert_eq<T: Num + FromStr + Debug>(&self, metrics_as_string: &str, expected_value: T) {
        let metric_value = self.parse_numeric_metric::<T>(metrics_as_string).unwrap();
        assert_equality(&metric_value, &expected_value, self.get_name(), None);
    }
}

/// An object which can be lossy converted into a `f64` representation.
pub trait LossyIntoF64 {
    fn into_f64(self) -> f64;
}

impl LossyIntoF64 for f64 {
    fn into_f64(self) -> f64 {
        self
    }
}

macro_rules! into_f64 {
    ($($ty:ty),*) => {
        $(
            impl LossyIntoF64 for $ty {
                #[allow(clippy::as_conversions)]
                fn into_f64(self) -> f64 {
                    self as f64
                }
            }
        )*
    };
}
into_f64!(u64, usize, i64, u128);

pub struct LabeledMetricGauge {
    scope: MetricScope,
    name: &'static str, // TODO(Tsabary): remove the _name_with_filter field, it is not used.
    description: &'static str,
    label_permutations: &'static [&'static [(&'static str, &'static str)]],
}

impl LabeledMetricGauge {
    pub const fn new(
        scope: MetricScope,
        name: &'static str,
        description: &'static str,
        label_permutations: &'static [&'static [(&'static str, &'static str)]],
    ) -> Self {
        Self { scope, name, description, label_permutations }
    }

    pub const fn get_name(&self) -> &'static str {
        self.name
    }

    pub fn get_name_with_filter(&self) -> String {
        format!("{}{}", self.name, metric_label_filter!())
    }

    pub const fn get_scope(&self) -> MetricScope {
        self.scope
    }

    pub const fn get_description(&self) -> &'static str {
        self.description
    }

    pub fn register(&self) {
        self.label_permutations.iter().map(|&slice| slice.to_vec()).for_each(|label| {
            let _ = gauge!(self.name, &label);
        });
        describe_gauge!(self.name, self.description);
    }

    pub fn increment<T: IntoF64>(&self, value: T, label: &[(&'static str, &'static str)]) {
        gauge!(self.name, label).increment(value);
    }

    pub fn decrement<T: IntoF64>(&self, value: T, label: &[(&'static str, &'static str)]) {
        gauge!(self.name, label).decrement(value.into_f64());
    }

    #[cfg(any(feature = "testing", test))]
    pub fn parse_numeric_metric<T: Num + FromStr>(
        &self,
        metrics_as_string: &str,
        label: &[(&'static str, &'static str)],
    ) -> Option<T> {
        parse_numeric_metric::<T>(metrics_as_string, self.get_name(), Some(label))
    }

    pub fn set<T: IntoF64>(&self, value: T, label: &[(&'static str, &'static str)]) {
        gauge!(self.name, label).set(value.into_f64());
    }

    #[cfg(any(feature = "testing", test))]
    #[track_caller]
    pub fn assert_eq<T: Num + FromStr + Debug>(
        &self,
        metrics_as_string: &str,
        expected_value: T,
        label: &[(&'static str, &'static str)],
    ) {
        let metric_value = self.parse_numeric_metric::<T>(metrics_as_string, label).unwrap();
        assert_equality(&metric_value, &expected_value, self.get_name(), Some(label));
    }
}

#[derive(Clone)]
pub struct MetricHistogram {
    scope: MetricScope,
    name: &'static str,
    description: &'static str,
}

#[derive(Default, Debug)]
pub struct HistogramValue {
    pub sum: f64,
    pub count: u64,
    pub histogram: IndexMap<String, f64>,
}

impl PartialEq for HistogramValue {
    fn eq(&self, other: &Self) -> bool {
        self.sum == other.sum && self.count == other.count
    }
}

impl MetricHistogram {
    pub const fn new(scope: MetricScope, name: &'static str, description: &'static str) -> Self {
        Self { scope, name, description }
    }

    pub const fn get_name(&self) -> &'static str {
        self.name
    }

    pub fn get_name_with_filter(&self) -> String {
        format!("{}_bucket{}", self.name, metric_label_filter!())
    }

    pub fn get_name_sum_with_filter(&self) -> String {
        format!("{}_sum{}", self.name, metric_label_filter!())
    }

    pub fn get_name_count_with_filter(&self) -> String {
        format!("{}_count{}", self.name, metric_label_filter!())
    }

    pub const fn get_scope(&self) -> MetricScope {
        self.scope
    }

    pub const fn get_description(&self) -> &'static str {
        self.description
    }

    pub fn register(&self) {
        let _ = histogram!(self.name);
        describe_histogram!(self.name, self.description);
    }

    pub fn record<T: IntoF64>(&self, value: T) {
        histogram!(self.name).record(value.into_f64());
    }

    pub fn record_lossy<T: LossyIntoF64>(&self, value: T) {
        histogram!(self.name).record(value.into_f64());
    }

    pub fn record_many<T: IntoF64>(&self, value: T, count: usize) {
        histogram!(self.name).record_many(value.into_f64(), count);
    }

    #[cfg(any(feature = "testing", test))]
    pub fn parse_histogram_metric(&self, metrics_as_string: &str) -> Option<HistogramValue> {
        parse_histogram_metric(metrics_as_string, self.get_name(), None)
    }

    #[cfg(any(feature = "testing", test))]
    #[track_caller]
    pub fn assert_eq(&self, metrics_as_string: &str, expected_value: &HistogramValue) {
        let metric_value = self.parse_histogram_metric(metrics_as_string).unwrap();
        assert_equality(&metric_value, expected_value, self.get_name(), None);
    }
}

pub struct LabeledMetricHistogram {
    scope: MetricScope,
    name: &'static str,
    description: &'static str,
    label_permutations: &'static [&'static [(&'static str, &'static str)]],
}

impl LabeledMetricHistogram {
    pub const fn new(
        scope: MetricScope,
        name: &'static str,
        description: &'static str,
        label_permutations: &'static [&'static [(&'static str, &'static str)]],
    ) -> Self {
        Self { scope, name, description, label_permutations }
    }

    pub const fn get_name(&self) -> &'static str {
        self.name
    }

    pub fn get_name_with_filter(&self) -> String {
        format!("{}_bucket{}", self.name, metric_label_filter!())
    }

    pub const fn get_scope(&self) -> MetricScope {
        self.scope
    }

    pub const fn get_description(&self) -> &'static str {
        self.description
    }

    // Returns a flattened and sorted list of the unique label values across all label permutations.
    // The flattening makes this mostly useful for a single labeled histograms, as otherwise
    // different domain values are mixed together.
    pub fn get_flat_label_values(&self) -> Vec<&str> {
        let mut values: Vec<&'static str> = self
            .label_permutations
            .iter()
            .flat_map(|pairs| pairs.iter().map(|(_, v)| *v))
            .collect();

        values.sort();
        values.dedup();
        values
    }

    pub fn register(&self) {
        self.label_permutations.iter().map(|&slice| slice.to_vec()).for_each(|labels| {
            let _ = histogram!(self.name, &labels);
        });
        describe_histogram!(self.name, self.description);
    }

    pub fn record<T: IntoF64>(&self, value: T, labels: &[(&'static str, &'static str)]) {
        histogram!(self.name, labels).record(value.into_f64());
    }

    pub fn record_many<T: IntoF64>(
        &self,
        value: T,
        count: usize,
        labels: &[(&'static str, &'static str)],
    ) {
        histogram!(self.name, labels).record_many(value.into_f64(), count);
    }

    #[cfg(any(feature = "testing", test))]
    pub fn parse_histogram_metric(
        &self,
        metrics_as_string: &str,
        labels: &[(&'static str, &'static str)],
    ) -> Option<HistogramValue> {
        parse_histogram_metric(metrics_as_string, self.get_name(), Some(labels))
    }

    #[cfg(any(feature = "testing", test))]
    #[track_caller]
    // TODO(tsabary): unite the labeled and unlabeld assert_eq functions.
    pub fn assert_eq(
        &self,
        metrics_as_string: &str,
        expected_value: &HistogramValue,
        label: &[(&'static str, &'static str)],
    ) {
        let metric_value = self.parse_histogram_metric(metrics_as_string, label).unwrap();
        assert_equality(&metric_value, expected_value, self.get_name(), Some(label));
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
#[cfg(any(feature = "testing", test))]
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

/// Parses a histogram metric from a metrics string.
///
/// # Arguments
///
/// - `metrics_as_string`: A string containing the rendered metrics data.
/// - `metric_name`: The name of the metric to search for.
/// - `labels`: Optional labels to match the metric.
///
/// # Returns
///
/// - `Option<HistogramValue>`: Returns `Some(HistogramValue)` if the metric is found and
///   successfully parsed. Returns `None` if the metric is not found or if parsing fails.
#[cfg(any(feature = "testing", test))]
pub fn parse_histogram_metric(
    metrics_as_string: &str,
    metric_name: &str,
    labels: Option<&[(&'static str, &'static str)]>,
) -> Option<HistogramValue> {
    // Construct a regex pattern to match the labels if provided.
    let mut quantile_labels_pattern = r#"\{"#.to_string();
    let mut labels_pattern = "".to_string();
    if let Some(labels) = labels {
        let inner_pattern = labels
            .iter()
            .map(|(k, v)| format!(r#"{}="{}""#, escape(k), escape(v)))
            .collect::<Vec<_>>()
            .join(r",");
        quantile_labels_pattern = format!(r#"\{{{},"#, inner_pattern);
        labels_pattern = format!(r#"\{{{}\}}"#, inner_pattern);
    }
    // Define regex patterns for quantiles, sum, and count.
    let quantile_pattern = format!(
        r#"{}{}quantile="([^"]+)"\}}\s+([\d\.]+)"#,
        escape(metric_name),
        quantile_labels_pattern
    );
    let sum_pattern = format!(r#"{}_sum{}\s+([\d\.]+)"#, escape(metric_name), labels_pattern);
    let count_pattern = format!(r#"{}_count{}\s+(\d+)"#, escape(metric_name), labels_pattern);

    // Compile the regex patterns.
    let quantile_re = Regex::new(&quantile_pattern).expect("Invalid regex for quantiles");
    let sum_re = Regex::new(&sum_pattern).expect("Invalid regex for sum");
    let count_re = Regex::new(&count_pattern).expect("Invalid regex for count");

    // Parse quantiles and insert them into the histogram.
    let mut histogram = IndexMap::new();
    for captures in quantile_re.captures_iter(metrics_as_string) {
        let quantile = captures.get(1)?.as_str().to_string();
        let value = captures.get(2)?.as_str().parse::<f64>().ok()?;
        histogram.insert(quantile, value);
    }

    // If no quantiles were found, return None.
    if histogram.is_empty() {
        return None;
    }

    // Parse the sum value.
    let sum = sum_re
        .captures(metrics_as_string)
        .and_then(|cap| cap.get(1))
        .and_then(|m| m.as_str().parse::<f64>().ok())
        .unwrap_or(0.0);

    // Parse the count value.
    let count = count_re
        .captures(metrics_as_string)
        .and_then(|cap| cap.get(1))
        .and_then(|m| m.as_str().parse::<u64>().ok())
        .unwrap_or(0);

    Some(HistogramValue { sum, count, histogram })
}

#[cfg(any(feature = "testing", test))]
fn assert_equality<T: PartialEq + Debug>(
    value: &T,
    expected_value: &T,
    metric_name: &str,
    label: Option<&[(&str, &str)]>,
) {
    let label_msg = label.map(|l| format!(" {:?}", l)).unwrap_or_default();
    assert_eq!(
        value, expected_value,
        "Metric {}{} did not match the expected value. Expected value: {:?}, metric value: {:?}",
        metric_name, label_msg, expected_value, value
    );
}
