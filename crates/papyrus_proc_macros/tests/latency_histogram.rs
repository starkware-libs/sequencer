use metrics::set_default_local_recorder;
use metrics_exporter_prometheus::PrometheusBuilder;
use papyrus_common::metrics::COLLECT_PROFILING_METRICS;
use papyrus_proc_macros::{latency_histogram, sequencer_latency_histogram};
use papyrus_test_utils::prometheus_is_contained;
use prometheus_parse::Value::Untyped;
use starknet_monitoring_endpoint::config::COLLECT_SEQUENCER_PROFILING_METRICS;
use starknet_sequencer_metrics::metrics::{MetricHistogram, MetricScope};

const FOO_HISTOGRAM_TEST_METRIC: MetricHistogram = MetricHistogram::new(
    MetricScope::Infra,
    "foo_histogram_test_metric",
    "foo function latency histogram test metrics",
);

const BAR_HISTOGRAM_TEST_METRIC: MetricHistogram = MetricHistogram::new(
    MetricScope::Infra,
    "bar_histogram_test_metric",
    "bar function latency histogram test metrics",
);

#[test]
fn latency_histogram_test() {
    COLLECT_PROFILING_METRICS.set(false).unwrap();

    #[latency_histogram("foo_histogram", false)]
    fn foo() -> usize {
        #[allow(clippy::let_and_return)]
        let start_function_time = 1000;
        start_function_time
    }

    #[latency_histogram("bar_histogram", true)]
    fn bar() -> usize {
        #[allow(clippy::let_and_return)]
        let start_function_time = 1000;
        start_function_time
    }

    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = set_default_local_recorder(&recorder);
    let handle = recorder.handle();

    assert!(handle.render().is_empty());
    assert_eq!(bar(), 1000);
    assert!(handle.render().is_empty());
    assert_eq!(foo(), 1000);
    assert_eq!(
        prometheus_is_contained(handle.render(), "foo_histogram_count", &[]),
        Some(Untyped(1f64))
    );
    // Test that the "start_function_time" variable from the macro is not shadowed.
    assert_ne!(
        prometheus_is_contained(handle.render(), "foo_histogram_sum", &[]),
        Some(Untyped(1000f64))
    );
}

#[test]
fn sequencer_latency_histogram_test() {
    let _ = COLLECT_SEQUENCER_PROFILING_METRICS.set(false);

    #[sequencer_latency_histogram(FOO_HISTOGRAM_TEST_METRIC, false)]
    fn foo() -> usize {
        #[allow(clippy::let_and_return)]
        let start_function_time = 1000;
        start_function_time
    }

    #[sequencer_latency_histogram(BAR_HISTOGRAM_TEST_METRIC, true)]
    fn bar() -> usize {
        #[allow(clippy::let_and_return)]
        let start_function_time = 1000;
        start_function_time
    }

    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = set_default_local_recorder(&recorder);
    let handle = recorder.handle();

    assert!(handle.render().is_empty());
    assert_eq!(bar(), 1000);
    assert!(handle.render().is_empty());
    assert_eq!(foo(), 1000);

    let count_metric = FOO_HISTOGRAM_TEST_METRIC.get_name().to_string() + "_count";
    assert_eq!(prometheus_is_contained(handle.render(), &count_metric, &[]), Some(Untyped(1f64)));
    // Test that the "start_function_time" variable from the macro is not shadowed.
    let sum_metric = FOO_HISTOGRAM_TEST_METRIC.get_name().to_string() + "_sum";
    assert_ne!(prometheus_is_contained(handle.render(), &sum_metric, &[]), Some(Untyped(1000f64)));
}
