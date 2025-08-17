use apollo_compile_to_casm_types::SIERRA_COMPILER_REQUEST_LABELS;
use apollo_metrics::define_metrics;

define_metrics!(
    CompileToCasm => {
        MetricHistogram { COMPILATION_DURATION, "compile_to_casm_compilation_duration_seconds", "Server-side compilation to casm duration in seconds" },
    },
    Infra => {
        LabeledMetricHistogram { SIERRA_COMPILER_LABELED_PROCESSING_TIMES_SECS, "sierra_compiler_labeled_processing_times_secs", "Request processing times of the sierra compiler, per label (secs)", labels = SIERRA_COMPILER_REQUEST_LABELS },
        LabeledMetricHistogram { SIERRA_COMPILER_LABELED_QUEUEING_TIMES_SECS, "sierra_compiler_labeled_queueing_times_secs", "Request queueing times of the sierra compiler, per label (secs)", labels = SIERRA_COMPILER_REQUEST_LABELS },
    },
);

pub(crate) fn register_metrics() {
    COMPILATION_DURATION.register();
}
