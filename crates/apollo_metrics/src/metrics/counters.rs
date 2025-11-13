use std::fmt::Debug;
#[cfg(any(feature = "testing", test))]
use std::str::FromStr;

use metrics::{counter, describe_counter};
#[cfg(any(feature = "testing", test))]
use num_traits::Num;

use crate::metrics::{
    HasMetricDetails,
    HasMetricFilterKind,
    Metric,
    MetricDetails,
    MetricFilterKind,
    MetricScope,
};
#[cfg(any(feature = "testing", test))]
use crate::test_utils::{assert_equality, assert_metric_exists, parse_numeric_metric};

impl HasMetricFilterKind for MetricCounter {
    const FILTER_KIND: MetricFilterKind = MetricFilterKind::CounterOrGauge;
}

impl HasMetricFilterKind for LabeledMetricCounter {
    const FILTER_KIND: MetricFilterKind = MetricFilterKind::CounterOrGauge;
}

pub struct MetricCounter {
    metric: Metric,
    initial_value: u64,
}

impl MetricCounter {
    pub const fn new(
        scope: MetricScope,
        name: &'static str,
        description: &'static str,
        initial_value: u64,
    ) -> Self {
        Self { metric: Metric::new(scope, name, description), initial_value }
    }

    pub fn register(&self) {
        counter!(self.get_name()).absolute(self.initial_value);
        describe_counter!(self.get_name(), self.get_description());
    }

    pub fn increment(&self, value: u64) {
        counter!(self.get_name()).increment(value);
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

    #[cfg(any(feature = "testing", test))]
    pub fn assert_exists(&self, metrics_as_string: &str) {
        assert_metric_exists(metrics_as_string, self.get_name(), "counter");
    }
}

impl HasMetricDetails for MetricCounter {
    type InnerMetricDetails = Metric;

    fn get_metric_description(&self) -> &Self::InnerMetricDetails {
        &self.metric
    }
}

pub struct LabeledMetricCounter {
    metric: Metric,
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
        Self { metric: Metric::new(scope, name, description), initial_value, label_permutations }
    }

    pub fn register(&self) {
        self.label_permutations.iter().map(|&slice| slice.to_vec()).for_each(|labels| {
            counter!(self.get_name(), &labels).absolute(self.initial_value);
        });
        describe_counter!(self.get_name(), self.get_description());
    }

    pub fn increment(&self, value: u64, labels: &[(&'static str, &'static str)]) {
        counter!(self.get_name(), labels).increment(value);
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

impl HasMetricDetails for LabeledMetricCounter {
    type InnerMetricDetails = Metric;

    fn get_metric_description(&self) -> &Self::InnerMetricDetails {
        &self.metric
    }
}
