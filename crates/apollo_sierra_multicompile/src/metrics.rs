use apollo_metrics::define_metrics;
use apollo_metrics::metrics::MetricHistogram;

define_metrics!(
    SierraMulticompile => {
        MetricHistogram { COMPILATION_LATENCY, "sierra_multicompile_compilation_latency", "Sierra compilation latency in seconds" },
    },
);

pub(crate) fn register_metrics() {
    COMPILATION_LATENCY.register();
}
