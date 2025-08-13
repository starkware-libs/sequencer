use apollo_metrics::define_metrics;

define_metrics!(
    CompileToCasm => {
        MetricHistogram { COMPILATION_DURATION, "compile_to_casm_compilation_duration_seconds", "Server-side compilation to casm duration in seconds" },
    },
);

pub(crate) fn register_metrics() {
    COMPILATION_DURATION.register();
}
