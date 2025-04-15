use apollo_metrics::define_metrics;
use apollo_metrics::metrics::MetricHistogram;

define_metrics!(
    CompileToCasm => {
        MetricHistogram { COMPILATION_LATENCY, "apollo_compile_to_casm_compilation_latency", "Compilation to casm latency in seconds" },
    },
);

pub(crate) fn register_metrics() {
    COMPILATION_LATENCY.register();
}
