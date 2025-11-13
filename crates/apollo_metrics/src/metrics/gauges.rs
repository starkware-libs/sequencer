use std::fmt::Debug;
#[cfg(any(feature = "testing", test))]
use std::str::FromStr;

use metrics::{describe_gauge, gauge, IntoF64};
#[cfg(any(feature = "testing", test))]
use num_traits::Num;

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
use crate::test_utils::{assert_equality, assert_metric_exists, parse_numeric_metric};

impl HasMetricFilterKind for MetricGauge {
    const FILTER_KIND: MetricFilterKind = MetricFilterKind::CounterOrGauge;
}

impl HasMetricFilterKind for LabeledMetricGauge {
    const FILTER_KIND: MetricFilterKind = MetricFilterKind::CounterOrGauge;
}

pub struct MetricGauge {
    metric: Metric,
}

impl MetricGauge {
    pub const fn new(scope: MetricScope, name: &'static str, description: &'static str) -> Self {
        Self { metric: Metric::new(scope, name, description) }
    }

    pub fn register(&self) {
        let _ = gauge!(self.get_name());
        describe_gauge!(self.get_name(), self.get_description());
    }

    pub fn increment<T: IntoF64>(&self, value: T) {
        gauge!(self.get_name()).increment(value.into_f64());
    }

    pub fn decrement<T: IntoF64>(&self, value: T) {
        gauge!(self.get_name()).decrement(value.into_f64());
    }

    #[cfg(any(feature = "testing", test))]
    pub fn parse_numeric_metric<T: Num + FromStr>(&self, metrics_as_string: &str) -> Option<T> {
        parse_numeric_metric::<T>(metrics_as_string, self.get_name(), None)
    }

    pub fn set<T: IntoF64>(&self, value: T) {
        gauge!(self.get_name()).set(value.into_f64());
    }

    pub fn set_lossy<T: LossyIntoF64>(&self, value: T) {
        gauge!(self.get_name()).set(value.into_f64());
    }

    #[cfg(any(feature = "testing", test))]
    #[track_caller]
    pub fn assert_eq<T: Num + FromStr + Debug>(&self, metrics_as_string: &str, expected_value: T) {
        let metric_value = self.parse_numeric_metric::<T>(metrics_as_string).unwrap();
        assert_equality(&metric_value, &expected_value, self.get_name(), None);
    }

    #[cfg(any(feature = "testing", test))]
    pub fn assert_exists(&self, metrics_as_string: &str) {
        assert_metric_exists(metrics_as_string, self.get_name(), "gauge");
    }
}

impl HasMetricDetails for MetricGauge {
    type InnerMetricDetails = Metric;

    fn get_metric_description(&self) -> &Self::InnerMetricDetails {
        &self.metric
    }
}

pub struct LabeledMetricGauge {
    metric: Metric,
    label_permutations: &'static [&'static [(&'static str, &'static str)]],
}

impl LabeledMetricGauge {
    pub const fn new(
        scope: MetricScope,
        name: &'static str,
        description: &'static str,
        label_permutations: &'static [&'static [(&'static str, &'static str)]],
    ) -> Self {
        Self { metric: Metric::new(scope, name, description), label_permutations }
    }

    pub fn register(&self) {
        self.label_permutations.iter().map(|&slice| slice.to_vec()).for_each(|label| {
            let _ = gauge!(self.get_name(), &label);
        });
        describe_gauge!(self.get_name(), self.get_description());
    }

    pub fn increment<T: IntoF64>(&self, value: T, label: &[(&'static str, &'static str)]) {
        gauge!(self.get_name(), label).increment(value);
    }

    pub fn decrement<T: IntoF64>(&self, value: T, label: &[(&'static str, &'static str)]) {
        gauge!(self.get_name(), label).decrement(value.into_f64());
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
        gauge!(self.get_name(), label).set(value.into_f64());
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

impl HasMetricDetails for LabeledMetricGauge {
    type InnerMetricDetails = Metric;

    fn get_metric_description(&self) -> &Self::InnerMetricDetails {
        &self.metric
    }
}
