use metrics::set_default_local_recorder;
use metrics_exporter_prometheus::PrometheusBuilder;
use rstest::rstest;
use strum::VariantNames;
use strum_macros::EnumVariantNames;

use crate::generate_permutation_labels;
use crate::metrics::{HistogramValue, LabeledMetricHistogram, MetricHistogram, MetricScope};

const HISTOGRAM_TEST_METRIC: MetricHistogram =
    MetricHistogram::new(MetricScope::Infra, "histogram_test_metric", "Histogram test metrics");

const LABEL_TYPE_NAME: &str = "label";
const VALUE_TYPE_NAME: &str = "value";

#[allow(dead_code)]
#[derive(Debug, EnumVariantNames, Clone, Copy)]
enum TestLabelType {
    Label1,
}

#[allow(dead_code)]
#[derive(Debug, EnumVariantNames, Clone, Copy)]
enum TestLabelValue {
    Value1,
    Value2,
}

// Create a labeled histogram metric with a single key-value pair.
generate_permutation_labels! {
    SINGLE_KEY_VALUE_PAIR_LABELS,
    (VALUE_TYPE_NAME, TestLabelValue),
}

const SINGLE_KEY_VALUE_PAIR_LABELED_HISTOGRAM_METRIC: LabeledMetricHistogram = LabeledMetricHistogram::new(
    MetricScope::Infra,
    "labeled_histogram_test_metric",
    "Labeled histogram test metrics",
    SINGLE_KEY_VALUE_PAIR_LABELS,
);

generate_permutation_labels! {
    TWO_KEY_VALUE_PAIR_LABELS,
    (LABEL_TYPE_NAME, TestLabelType),
    (VALUE_TYPE_NAME, TestLabelValue),
}

const TWO_KEY_VALUE_PAIR_LABELED_HISTOGRAM_METRIC: LabeledMetricHistogram = LabeledMetricHistogram::new(
    MetricScope::Infra,
    "multi_labeled_histogram_test_metric",
    "Multi labeled histogram test metrics",
    TWO_KEY_VALUE_PAIR_LABELS,
);

#[test]
fn histogram_run_and_parse() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = set_default_local_recorder(&recorder);

    HISTOGRAM_TEST_METRIC.register();
    HISTOGRAM_TEST_METRIC.record(1);
    HISTOGRAM_TEST_METRIC.record(100);
    HISTOGRAM_TEST_METRIC.record(80);
    HISTOGRAM_TEST_METRIC.record(50);
    HISTOGRAM_TEST_METRIC.record_many(93, 1);

    let metrics_as_string = recorder.handle().render();

    let quantiles = vec![
        ("0".to_string(), 1.0),
        ("0.5".to_string(), 80.00587021001003),
        ("0.9".to_string(), 92.99074853701167),
        ("0.95".to_string(), 92.99074853701167),
        ("0.99".to_string(), 92.99074853701167),
        ("0.999".to_string(), 92.99074853701167),
        ("1".to_string(), 100.0),
    ];
    let expected_histogram = HistogramValue {
        sum: 324.0,
        count: 5,
        histogram: quantiles.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
    };

    assert_eq!(
        HISTOGRAM_TEST_METRIC.parse_histogram_metric(&metrics_as_string).unwrap(),
        expected_histogram
    );
}

#[rstest]
#[case::single_key_value_pair(SINGLE_KEY_VALUE_PAIR_LABELED_HISTOGRAM_METRIC, SINGLE_KEY_VALUE_PAIR_LABELS)]
#[case::two_key_value_pair(TWO_KEY_VALUE_PAIR_LABELED_HISTOGRAM_METRIC, TWO_KEY_VALUE_PAIR_LABELS)]
#[test]
fn labeled_histogram_run_and_parse(#[case] labeled_histogram_metric: LabeledMetricHistogram,#[case] labels: &'static [&[(&str, &str)]]) {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = set_default_local_recorder(&recorder);

    // Let perform some actions for the histogram metric with labels[0].
    labeled_histogram_metric.register();
    labeled_histogram_metric.record(1, labels[0]);
    labeled_histogram_metric.record(100, labels[0]);
    labeled_histogram_metric.record(80, labels[0]);
    labeled_histogram_metric.record(50, labels[0]);
    labeled_histogram_metric.record_many(93, 1, labels[0]);

    let metrics_as_string = recorder.handle().render();

    let label1_quantiles = vec![
        ("0".to_string(), 1.0),
        ("0.5".to_string(), 80.00587021001003),
        ("0.9".to_string(), 92.99074853701167),
        ("0.95".to_string(), 92.99074853701167),
        ("0.99".to_string(), 92.99074853701167),
        ("0.999".to_string(), 92.99074853701167),
        ("1".to_string(), 100.0),
    ];
    let label1_expected_histogram = HistogramValue {
        sum: 324.0,
        count: 5,
        histogram: label1_quantiles.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
    };

    assert_eq!(
        labeled_histogram_metric.parse_histogram_metric(&metrics_as_string, labels[0]).unwrap(),
        label1_expected_histogram
    );

    // The histogram metric with labels[1] should be empty.
    let label2_quantiles = vec![
        ("0".to_string(), 0.0),
        ("0.5".to_string(), 0.0),
        ("0.9".to_string(), 0.0),
        ("0.95".to_string(), 0.0),
        ("0.99".to_string(), 0.0),
        ("0.999".to_string(), 0.0),
        ("1".to_string(), 0.0),
    ];
    let label2_expected_histogram = HistogramValue {
        sum: 0.0,
        count: 0,
        histogram: label2_quantiles.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
    };

    assert_eq!(
        labeled_histogram_metric.parse_histogram_metric(&metrics_as_string, labels[1]).unwrap(),
        label2_expected_histogram
    );

    // Let perform some actions for the histogram metric with labels[1].
    labeled_histogram_metric.record(1, labels[1]);
    labeled_histogram_metric.record(10, labels[1]);
    labeled_histogram_metric.record(20, labels[1]);
    labeled_histogram_metric.record(30, labels[1]);
    labeled_histogram_metric.record_many(80, 2, labels[1]);

    let metrics_as_string = recorder.handle().render();

    // The histogram metric with labels[0] should be the same.
    assert_eq!(
        labeled_histogram_metric.parse_histogram_metric(&metrics_as_string, labels[0]).unwrap(),
        label1_expected_histogram
    );

    let label2_quantiles = vec![
        ("0".to_string(), 1.0),
        ("0.5".to_string(), 19.999354639046004),
        ("0.9".to_string(), 80.00587021001003),
        ("0.95".to_string(), 80.00587021001003),
        ("0.99".to_string(), 80.00587021001003),
        ("0.999".to_string(), 80.00587021001003),
        ("1".to_string(), 80.0),
    ];
    let label2_expected_histogram = HistogramValue {
        sum: 221.0,
        count: 6,
        histogram: label2_quantiles.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
    };

    // Check the histogram metric with labels[1].
    assert_eq!(
        labeled_histogram_metric.parse_histogram_metric(&metrics_as_string, labels[1]).unwrap(),
        label2_expected_histogram
    );
}
