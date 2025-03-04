use metrics::set_default_local_recorder;
use metrics_exporter_prometheus::PrometheusBuilder;

use crate::metrics::{HistogramValue, LabeledMetricHistogram, MetricHistogram, MetricScope};

const HISTOGRAM_TEST_METRIC: MetricHistogram =
    MetricHistogram::new(MetricScope::Infra, "histogram_test_metric", "Histogram test metrics");

const LABEL: &[(&str, &str)] = &[("label1", "value1")];

const LABELED_HISTOGRAM_TEST_METRIC: LabeledMetricHistogram = LabeledMetricHistogram::new(
    MetricScope::Infra,
    "labeled_histogram_test_metric",
    "Labeled histogram test metrics",
    &[LABEL]
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

    LABELED_HISTOGRAM_TEST_METRIC.register();
    LABELED_HISTOGRAM_TEST_METRIC.record(1, LABEL);
    LABELED_HISTOGRAM_TEST_METRIC.record(100, LABEL);
    LABELED_HISTOGRAM_TEST_METRIC.record(80, LABEL);
    LABELED_HISTOGRAM_TEST_METRIC.record(50, LABEL);
    LABELED_HISTOGRAM_TEST_METRIC.record_many(93, 1, LABEL);

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
        LABELED_HISTOGRAM_TEST_METRIC.parse_histogram_metric(&metrics_as_string, LABEL).unwrap(),
        expected_histogram
    );
}
