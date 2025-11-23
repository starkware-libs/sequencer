use std::sync::OnceLock;

use apollo_metrics::metrics::{MetricHistogram, MetricScope, COLLECT_SEQUENCER_PROFILING_METRICS};
use apollo_proc_macros::{latency_histogram, sequencer_latency_histogram};
use apollo_test_utils::prometheus_is_contained;
use metrics::set_default_local_recorder;
use metrics_exporter_prometheus::PrometheusBuilder;
use papyrus_common::metrics::COLLECT_PROFILING_METRICS;
use prometheus_parse::Value::Untyped;
use rstest::rstest;

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

type TestFn = fn() -> usize;

#[latency_histogram("foo_histogram", false)]
fn foo_for_papyrus_macro() -> usize {
    #[allow(clippy::let_and_return)]
    let start_function_time = 1000;
    start_function_time
}

#[latency_histogram("bar_histogram", true)]
fn bar_for_papyrus_macro() -> usize {
    #[allow(clippy::let_and_return)]
    let start_function_time = 1000;
    start_function_time
}

#[sequencer_latency_histogram(FOO_HISTOGRAM_TEST_METRIC, false)]
fn foo_for_sequencer_macro() -> usize {
    #[allow(clippy::let_and_return)]
    let start_function_time = 1000;
    start_function_time
}

#[sequencer_latency_histogram(BAR_HISTOGRAM_TEST_METRIC, true)]
fn bar_for_sequencer_macro() -> usize {
    #[allow(clippy::let_and_return)]
    let start_function_time = 1000;
    start_function_time
}

#[rstest]
#[case::latency_histogram(
    &COLLECT_PROFILING_METRICS,
    foo_for_papyrus_macro,
    bar_for_papyrus_macro,
    "foo_histogram_count",
    "foo_histogram_sum"
)]
#[case::sequencer_latency_histogram(
    &COLLECT_SEQUENCER_PROFILING_METRICS,
    foo_for_sequencer_macro,
    bar_for_sequencer_macro,
    FOO_HISTOGRAM_TEST_METRIC.get_name().to_string() + "_count",
    FOO_HISTOGRAM_TEST_METRIC.get_name().to_string() + "_sum"
)]
fn latency_histogram_test(
    #[case] global_flag: &OnceLock<bool>,
    #[case] foo: TestFn,
    #[case] bar: TestFn,
    #[case] count_metric: String,
    #[case] sum_metric: String,
) {
    global_flag.set(false).unwrap();

    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = set_default_local_recorder(&recorder);
    let handle = recorder.handle();

    assert!(handle.render().is_empty());
    assert_eq!(bar(), 1000);
    assert!(handle.render().is_empty());
    assert_eq!(foo(), 1000);

    assert_eq!(prometheus_is_contained(handle.render(), &count_metric, &[]), Some(Untyped(1f64)));
    // Test that the "start_function_time" variable from the macro is not shadowed.
    assert_ne!(prometheus_is_contained(handle.render(), &sum_metric, &[]), Some(Untyped(1000f64)));
}
