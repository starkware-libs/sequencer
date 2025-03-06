use metrics::set_default_local_recorder;
use metrics_exporter_prometheus::PrometheusBuilder;
use strum::VariantNames;
use strum_macros::EnumVariantNames;

use crate::generate_permutation_labels;
use crate::metrics::{HistogramValue, LabeledMetricHistogram, MetricHistogram, MetricScope};

const HISTOGRAM_TEST_METRIC: MetricHistogram =
    MetricHistogram::new(MetricScope::Infra, "histogram_test_metric", "Histogram test metrics");

const LABEL1: &[(&str, &str)] = &[("label1", "value1")];
const LABEL2: &[(&str, &str)] = &[("label1", "value2")];

const LABELED_HISTOGRAM_TEST_METRIC: LabeledMetricHistogram = LabeledMetricHistogram::new(
    MetricScope::Infra,
    "labeled_histogram_test_metric",
    "Labeled histogram test metrics",
    &[LABEL1, LABEL2],
);

const LABEL_NAME_TYPE: &str = "test_type";
const LABEL_NAME_SOURCE: &str = "source";

#[allow(dead_code)]
#[derive(Debug, EnumVariantNames, Clone, Copy)]
enum TestLabelType {
    Type1,
    Type2,
}

#[allow(dead_code)]
#[derive(Debug, EnumVariantNames, Clone, Copy)]
enum TestLabelSource {
    Source1,
    Source2,
}

generate_permutation_labels! {
    TEST_TYPE_AND_SOURCE_LABELS,
    (LABEL_NAME_TYPE, TestLabelType),
    (LABEL_NAME_SOURCE, TestLabelSource),
}

const MULTI_LABELED_HISTOGRAM_TEST_METRIC: LabeledMetricHistogram = LabeledMetricHistogram::new(
    MetricScope::Infra,
    "multi_labeled_histogram_test_metric",
    "Multi labeled histogram test metrics",
    TEST_TYPE_AND_SOURCE_LABELS,
);

enum TestHistogramActionSet {
    Empty,
    Set1,
    Set2,
}

fn expected_histogram_value(action_set: TestHistogramActionSet) -> HistogramValue {
    match action_set {
        TestHistogramActionSet::Empty => {
            let quantiles = vec![
                ("0".to_string(), 0.0),
                ("0.5".to_string(), 0.0),
                ("0.9".to_string(), 0.0),
                ("0.95".to_string(), 0.0),
                ("0.99".to_string(), 0.0),
                ("0.999".to_string(), 0.0),
                ("1".to_string(), 0.0),
            ];
            HistogramValue {
                sum: 0.0,
                count: 0,
                histogram: quantiles.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
            }
        }
        TestHistogramActionSet::Set1 => {
            let quantiles = vec![
                ("0".to_string(), 1.0),
                ("0.5".to_string(), 80.00587021001003),
                ("0.9".to_string(), 92.99074853701167),
                ("0.95".to_string(), 92.99074853701167),
                ("0.99".to_string(), 92.99074853701167),
                ("0.999".to_string(), 92.99074853701167),
                ("1".to_string(), 100.0),
            ];
            HistogramValue {
                sum: 324.0,
                count: 5,
                histogram: quantiles.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
            }
        }
        TestHistogramActionSet::Set2 => {
            let quantiles = vec![
                ("0".to_string(), 1.0),
                ("0.5".to_string(), 19.999354639046004),
                ("0.9".to_string(), 80.00587021001003),
                ("0.95".to_string(), 80.00587021001003),
                ("0.99".to_string(), 80.00587021001003),
                ("0.999".to_string(), 80.00587021001003),
                ("1".to_string(), 80.0),
            ];
            HistogramValue {
                sum: 221.0,
                count: 6,
                histogram: quantiles.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
            }
        }
    }
}

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

    assert_eq!(
        HISTOGRAM_TEST_METRIC.parse_histogram_metric(&metrics_as_string).unwrap(),
        expected_histogram_value(TestHistogramActionSet::Set1)
    );
}

fn perform_action_set_for_labeled_histogram(
    metric: &LabeledMetricHistogram,
    labels: &'static [(&str, &str)],
    action_set: TestHistogramActionSet,
) {
    match action_set {
        TestHistogramActionSet::Empty => {}
        TestHistogramActionSet::Set1 => {
            metric.record(1, labels);
            metric.record(100, labels);
            metric.record(80, labels);
            metric.record(50, labels);
            metric.record_many(93, 1, labels);
        }
        TestHistogramActionSet::Set2 => {
            metric.record(1, labels);
            metric.record(10, labels);
            metric.record(20, labels);
            metric.record(30, labels);
            metric.record_many(80, 2, labels);
        }
    }
}

#[test]
fn labeled_histogram_run_and_parse() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = set_default_local_recorder(&recorder);

    // Let perform action set 1 for the histogram metric with LABEL1.
    LABELED_HISTOGRAM_TEST_METRIC.register();
    perform_action_set_for_labeled_histogram(
        &LABELED_HISTOGRAM_TEST_METRIC,
        LABEL1,
        TestHistogramActionSet::Set1,
    );

    let metrics_as_string = recorder.handle().render();

    assert_eq!(
        LABELED_HISTOGRAM_TEST_METRIC.parse_histogram_metric(&metrics_as_string, LABEL1).unwrap(),
        expected_histogram_value(TestHistogramActionSet::Set1)
    );

    // The histogram metric with LABEL2 should be empty.
    assert_eq!(
        LABELED_HISTOGRAM_TEST_METRIC.parse_histogram_metric(&metrics_as_string, LABEL2).unwrap(),
        expected_histogram_value(TestHistogramActionSet::Empty)
    );

    // Let perform action set 2 for the histogram metric with LABEL2.
    perform_action_set_for_labeled_histogram(
        &LABELED_HISTOGRAM_TEST_METRIC,
        LABEL2,
        TestHistogramActionSet::Set2,
    );

    let metrics_as_string = recorder.handle().render();

    // The histogram metric with LABEL1 should be the same.
    assert_eq!(
        LABELED_HISTOGRAM_TEST_METRIC.parse_histogram_metric(&metrics_as_string, LABEL1).unwrap(),
        expected_histogram_value(TestHistogramActionSet::Set1)
    );
    // Check the histogram metric with LABEL2.
    assert_eq!(
        LABELED_HISTOGRAM_TEST_METRIC.parse_histogram_metric(&metrics_as_string, LABEL2).unwrap(),
        expected_histogram_value(TestHistogramActionSet::Set2)
    );
}

fn create_expected_empty_histogram_values_vec(len: usize) -> Vec<HistogramValue> {
    let expected_vec: Vec<HistogramValue> =
        (0..len).map(|_| expected_histogram_value(TestHistogramActionSet::Empty)).collect();
    expected_vec
}

fn create_expected_histogram_values_vec(
    len: usize,
    index1: usize,
    action_set1: TestHistogramActionSet,
    index2: usize,
    action_set2: TestHistogramActionSet,
) -> Vec<HistogramValue> {
    // Initialize the expected_histogram vector with empty values.
    let mut expected_vec = create_expected_empty_histogram_values_vec(len);

    // Replace the values at the specified indices with the provided values.
    if index1 < len {
        expected_vec[index1] = expected_histogram_value(action_set1);
    }
    if index2 < len {
        expected_vec[index2] = expected_histogram_value(action_set2);
    }

    expected_vec
}

fn compare_histogram(
    metrics_as_string: &str,
    expected_histogram: &[HistogramValue],
    labels: &'static [&[(&str, &str)]],
) {
    labels.iter().zip(expected_histogram).for_each(|(labels, expected_histogram)| {
        assert_eq!(
            MULTI_LABELED_HISTOGRAM_TEST_METRIC
                .parse_histogram_metric(metrics_as_string, labels)
                .unwrap(),
            *expected_histogram
        );
    });
}

#[test]
fn multi_labeled_histogram_run_and_parse() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = set_default_local_recorder(&recorder);

    let labels_len = TEST_TYPE_AND_SOURCE_LABELS.len();

    MULTI_LABELED_HISTOGRAM_TEST_METRIC.register();

    let metrics_as_string = recorder.handle().render();

    // The histogram metric for all labels should be empty.
    compare_histogram(
        &metrics_as_string,
        &create_expected_empty_histogram_values_vec(labels_len),
        TEST_TYPE_AND_SOURCE_LABELS,
    );

    // Let perform actions set 1 for the histogram metric with label TEST_TYPE_AND_SOURCE_LABELS[0].
    let index1 = 0;
    perform_action_set_for_labeled_histogram(
        &MULTI_LABELED_HISTOGRAM_TEST_METRIC,
        TEST_TYPE_AND_SOURCE_LABELS[index1],
        TestHistogramActionSet::Set1,
    );

    let metrics_as_string = recorder.handle().render();

    let expected_histogram = create_expected_histogram_values_vec(
        labels_len,
        index1,
        TestHistogramActionSet::Set1,
        labels_len + 1,
        TestHistogramActionSet::Empty,
    );

    compare_histogram(&metrics_as_string, &expected_histogram, TEST_TYPE_AND_SOURCE_LABELS);

    let index2 = TEST_TYPE_AND_SOURCE_LABELS.len() / 2;
    // Let perform actions set 2 for the histogram metric with label
    // TEST_TYPE_AND_SOURCE_LABELS[index2].
    perform_action_set_for_labeled_histogram(
        &MULTI_LABELED_HISTOGRAM_TEST_METRIC,
        TEST_TYPE_AND_SOURCE_LABELS[index2],
        TestHistogramActionSet::Set2,
    );

    let metrics_as_string = recorder.handle().render();

    let expected_histogram = create_expected_histogram_values_vec(
        labels_len,
        index1,
        TestHistogramActionSet::Set1,
        index2,
        TestHistogramActionSet::Set2,
    );

    compare_histogram(&metrics_as_string, &expected_histogram, TEST_TYPE_AND_SOURCE_LABELS);
}
