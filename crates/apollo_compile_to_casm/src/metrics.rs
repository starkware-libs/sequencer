use apollo_compile_to_casm_types::SIERRA_COMPILER_REQUEST_LABELS;
use apollo_infra::metrics::{
    InfraMetrics,
    LocalClientMetrics,
    LocalServerMetrics,
    RemoteClientMetrics,
    RemoteServerMetrics,
};
use apollo_metrics::{define_infra_metrics, define_metrics};

define_infra_metrics!(sierra_compiler);

define_metrics!(
    CompileToCasm => {
        MetricHistogram { COMPILATION_DURATION, "compile_to_casm_compilation_duration_seconds", "Server-side compilation to casm duration in seconds" },
    },
);

pub(crate) fn register_metrics() {
    COMPILATION_DURATION.register();
}
