use metrics::set_default_local_recorder;
use metrics_exporter_prometheus::PrometheusBuilder;

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

#[test]
fn labeled_histogram_run_and_parse() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = set_default_local_recorder(&recorder);

    // Let perform some actions for the histogram metric with LABEL1.
    LABELED_HISTOGRAM_TEST_METRIC.register();
    LABELED_HISTOGRAM_TEST_METRIC.record(1, LABEL1);
    LABELED_HISTOGRAM_TEST_METRIC.record(100, LABEL1);
    LABELED_HISTOGRAM_TEST_METRIC.record(80, LABEL1);
    LABELED_HISTOGRAM_TEST_METRIC.record(50, LABEL1);
    LABELED_HISTOGRAM_TEST_METRIC.record_many(93, 1, LABEL1);

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
        LABELED_HISTOGRAM_TEST_METRIC.parse_histogram_metric(&metrics_as_string, LABEL1).unwrap(),
        label1_expected_histogram
    );

    // The histogram metric with LABEL2 should be empty.
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
        LABELED_HISTOGRAM_TEST_METRIC.parse_histogram_metric(&metrics_as_string, LABEL2).unwrap(),
        label2_expected_histogram
    );

    // Let perform some actions for the histogram metric with LABEL2.
    LABELED_HISTOGRAM_TEST_METRIC.record(1, LABEL2);
    LABELED_HISTOGRAM_TEST_METRIC.record(10, LABEL2);
    LABELED_HISTOGRAM_TEST_METRIC.record(20, LABEL2);
    LABELED_HISTOGRAM_TEST_METRIC.record(30, LABEL2);
    LABELED_HISTOGRAM_TEST_METRIC.record_many(80, 2, LABEL2);

    let metrics_as_string = recorder.handle().render();

    // The histogram metric with LABEL1 should be the same.
    assert_eq!(
        LABELED_HISTOGRAM_TEST_METRIC.parse_histogram_metric(&metrics_as_string, LABEL1).unwrap(),
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

    // Check the histogram metric with LABEL2.
    assert_eq!(
        LABELED_HISTOGRAM_TEST_METRIC.parse_histogram_metric(&metrics_as_string, LABEL2).unwrap(),
        label2_expected_histogram
    );
}
