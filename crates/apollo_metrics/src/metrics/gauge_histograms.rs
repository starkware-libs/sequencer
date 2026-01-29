use metrics::{describe_gauge, describe_histogram, gauge, histogram, IntoF64};

use crate::metrics::{HasMetricDetails, LossyIntoF64, Metric, MetricDetails, MetricScope};

const GAUGE_SUFFIX: &str = "_gauge";
const HISTOGRAM_SUFFIX: &str = "_hist";

/// A metric that combines both a gauge and a histogram.
/// The gauge tracks the current value (with "_gauge" suffix), while the histogram
/// tracks the distribution over time (with "_hist" suffix).
pub struct MetricGaugeHistogram {
    metric: Metric,
}

impl MetricGaugeHistogram {
    pub const fn new(scope: MetricScope, name: &'static str, description: &'static str) -> Self {
        Self { metric: Metric::new(scope, name, description) }
    }

    pub fn register(&self) {
        let _ = gauge!(self.get_gauge_name());
        describe_gauge!(self.get_gauge_name(), self.get_description());

        let _ = histogram!(self.get_histogram_name());
        describe_histogram!(self.get_histogram_name(), self.get_description());
    }

    pub fn get_gauge_name(&self) -> String {
        format!("{}{GAUGE_SUFFIX}", self.metric.get_name())
    }

    pub fn get_histogram_name(&self) -> String {
        format!("{}{HISTOGRAM_SUFFIX}", self.metric.get_name())
    }

    /// Sets the gauge value and records it in the histogram.
    pub fn set<T: IntoF64 + Copy>(&self, value: T) {
        gauge!(self.get_gauge_name()).set(value.into_f64());
        histogram!(self.get_histogram_name()).record(value.into_f64());
    }

    /// Sets the gauge value and records it in the histogram (lossy conversion).
    pub fn set_lossy<T: LossyIntoF64 + Copy>(&self, value: T) {
        gauge!(self.get_gauge_name()).set(value.into_f64());
        histogram!(self.get_histogram_name()).record(value.into_f64());
    }
}

impl HasMetricDetails for MetricGaugeHistogram {
    type InnerMetricDetails = Metric;

    fn get_metric_description(&self) -> &Self::InnerMetricDetails {
        &self.metric
    }
}
