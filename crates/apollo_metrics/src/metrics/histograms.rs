use std::collections::BTreeSet;
use std::fmt::Debug;

use indexmap::IndexMap;
use metrics::{describe_histogram, histogram, IntoF64};
#[cfg(any(feature = "testing", test))]
use regex::{escape, Regex};

use crate::metric_definitions::METRIC_LABEL_FILTER;
use crate::metrics::{
    HasMetricDetails,
    HasMetricFilterKind,
    LossyIntoF64,
    Metric,
    MetricDetails,
    MetricFilterKind,
    MetricScope,
};
#[cfg(any(feature = "testing", test))]
use crate::test_utils::assert_equality;

impl HasMetricFilterKind for MetricHistogram {
    const FILTER_KIND: MetricFilterKind = MetricFilterKind::Histogram;
}

impl HasMetricFilterKind for LabeledMetricHistogram {
    const FILTER_KIND: MetricFilterKind = MetricFilterKind::Histogram;
}

#[derive(Clone)]
pub struct MetricHistogram {
    metric: Metric,
}

#[derive(Default, Debug)]
pub struct HistogramValue {
    pub sum: f64,
    pub count: u64,
    // TODO(Tsabary): why is this set with an index map? Consider alternatives.
    pub histogram: IndexMap<String, f64>,
}

impl PartialEq for HistogramValue {
    fn eq(&self, other: &Self) -> bool {
        self.sum == other.sum && self.count == other.count
    }
}

impl MetricHistogram {
    pub const fn new(scope: MetricScope, name: &'static str, description: &'static str) -> Self {
        Self { metric: Metric::new(scope, name, description) }
    }

    pub fn get_name_sum_with_filter(&self) -> String {
        format!("{}_sum{METRIC_LABEL_FILTER}", self.get_name())
    }

    pub fn get_name_count_with_filter(&self) -> String {
        format!("{}_count{METRIC_LABEL_FILTER}", self.get_name())
    }

    pub fn register(&self) {
        let _ = histogram!(self.get_name());
        describe_histogram!(self.get_name(), self.get_description());
    }

    pub fn record<T: IntoF64>(&self, value: T) {
        histogram!(self.get_name()).record(value.into_f64());
    }

    pub fn record_lossy<T: LossyIntoF64>(&self, value: T) {
        histogram!(self.get_name()).record(value.into_f64());
    }

    pub fn record_many<T: IntoF64>(&self, value: T, count: usize) {
        histogram!(self.get_name()).record_many(value.into_f64(), count);
    }

    #[cfg(any(feature = "testing", test))]
    pub(crate) fn parse_histogram_metric(&self, metrics_as_string: &str) -> Option<HistogramValue> {
        parse_histogram_metric(metrics_as_string, self.get_name(), None)
    }

    #[cfg(any(feature = "testing", test))]
    #[track_caller]
    pub fn assert_eq(&self, metrics_as_string: &str, expected_value: &HistogramValue) {
        let metric_value = self.parse_histogram_metric(metrics_as_string).unwrap();
        assert_equality(&metric_value, expected_value, self.get_name(), None);
    }
}

impl HasMetricDetails for MetricHistogram {
    type InnerMetricDetails = Metric;

    fn get_metric_description(&self) -> &Self::InnerMetricDetails {
        &self.metric
    }
}

pub struct LabeledMetricHistogram {
    metric: Metric,
    label_permutations: &'static [&'static [(&'static str, &'static str)]],
}

impl LabeledMetricHistogram {
    pub const fn new(
        scope: MetricScope,
        name: &'static str,
        description: &'static str,
        label_permutations: &'static [&'static [(&'static str, &'static str)]],
    ) -> Self {
        Self { metric: Metric::new(scope, name, description), label_permutations }
    }

    // Returns a flattened and sorted list of the unique label values across all label permutations.
    // The flattening makes this mostly useful for a single labeled histograms, as otherwise
    // different domain values are mixed together.
    pub fn get_flat_label_values(&self) -> Vec<&str> {
        self
            .label_permutations
            .iter()
            .flat_map(|pairs| pairs.iter().map(|(_, v)| *v))
               .collect::<BTreeSet<_>>()   // unique + sorted
        .into_iter()
    .collect()
    }

    pub fn register(&self) {
        self.label_permutations.iter().map(|&slice| slice.to_vec()).for_each(|labels| {
            let _ = histogram!(self.get_name(), &labels);
        });
        describe_histogram!(self.get_name(), self.get_description());
    }

    /// Returns the label name used by this labeled histogram.
    pub fn get_label_name(&self) -> &'static str {
        self.label_permutations[0][0].0
    }

    pub fn record<T: IntoF64>(&self, value: T, labels: &[(&'static str, &'static str)]) {
        histogram!(self.get_name(), labels).record(value.into_f64());
    }

    pub fn record_many<T: IntoF64>(
        &self,
        value: T,
        count: usize,
        labels: &[(&'static str, &'static str)],
    ) {
        histogram!(self.get_name(), labels).record_many(value.into_f64(), count);
    }

    #[cfg(any(feature = "testing", test))]
    pub(crate) fn parse_histogram_metric(
        &self,
        metrics_as_string: &str,
        labels: &[(&'static str, &'static str)],
    ) -> Option<HistogramValue> {
        parse_histogram_metric(metrics_as_string, self.get_name(), Some(labels))
    }

    #[cfg(any(feature = "testing", test))]
    #[track_caller]
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

impl HasMetricDetails for LabeledMetricHistogram {
    type InnerMetricDetails = Metric;

    fn get_metric_description(&self) -> &Self::InnerMetricDetails {
        &self.metric
    }
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
pub(crate) fn parse_histogram_metric(
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
        quantile_labels_pattern = format!(r#"\{{{inner_pattern},"#);
        labels_pattern = format!(r#"\{{{inner_pattern}\}}"#);
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
