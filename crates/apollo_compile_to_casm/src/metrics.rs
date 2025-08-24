use apollo_compile_to_casm_types::SIERRA_COMPILER_REQUEST_LABELS;
use apollo_infra::metrics::{
    InfraMetrics,
    LocalClientMetrics,
    LocalServerMetrics,
    RemoteClientMetrics,
    RemoteServerMetrics,
    SIERRA_COMPILER_LOCAL_MSGS_PROCESSED,
    SIERRA_COMPILER_LOCAL_MSGS_RECEIVED,
    SIERRA_COMPILER_LOCAL_QUEUE_DEPTH,
    SIERRA_COMPILER_REMOTE_CLIENT_SEND_ATTEMPTS,
    SIERRA_COMPILER_REMOTE_MSGS_PROCESSED,
    SIERRA_COMPILER_REMOTE_MSGS_RECEIVED,
    SIERRA_COMPILER_REMOTE_NUMBER_OF_CONNECTIONS,
    SIERRA_COMPILER_REMOTE_VALID_MSGS_RECEIVED,
};
use apollo_metrics::define_metrics;

define_metrics!(
    CompileToCasm => {
        MetricHistogram { COMPILATION_DURATION, "compile_to_casm_compilation_duration_seconds", "Server-side compilation to casm duration in seconds" },
    },
    Infra => {
        LabeledMetricHistogram { SIERRA_COMPILER_LABELED_PROCESSING_TIMES_SECS, "sierra_compiler_labeled_processing_times_secs", "Request processing times of the sierra compiler, per label (secs)", labels = SIERRA_COMPILER_REQUEST_LABELS },
        LabeledMetricHistogram { SIERRA_COMPILER_LABELED_QUEUEING_TIMES_SECS, "sierra_compiler_labeled_queueing_times_secs", "Request queueing times of the sierra compiler, per label (secs)", labels = SIERRA_COMPILER_REQUEST_LABELS },
        LabeledMetricHistogram { SIERRA_COMPILER_LABELED_LOCAL_RESPONSE_TIMES_SECS, "sierra_compiler_labeled_local_response_times_secs", "Request local response times of the sierra compiler, per label (secs)", labels = SIERRA_COMPILER_REQUEST_LABELS },
        LabeledMetricHistogram { SIERRA_COMPILER_LABELED_REMOTE_RESPONSE_TIMES_SECS, "sierra_compiler_labeled_remote_response_times_secs", "Request remote response times of the sierra compiler, per label (secs)", labels = SIERRA_COMPILER_REQUEST_LABELS },
        LabeledMetricHistogram { SIERRA_COMPILER_LABELED_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES_SECS, "sierra_compiler_labeled_remote_client_communication_failure_times_secs", "Request communication failure times of the sierra compiler, per label (secs)", labels = SIERRA_COMPILER_REQUEST_LABELS },
    },
);

pub(crate) fn register_metrics() {
    COMPILATION_DURATION.register();
}

pub const _SIERRA_COMPILER_INFRA_METRICS: InfraMetrics = InfraMetrics::new(
    LocalClientMetrics::new(&SIERRA_COMPILER_LABELED_LOCAL_RESPONSE_TIMES_SECS),
    RemoteClientMetrics::new(
        &SIERRA_COMPILER_REMOTE_CLIENT_SEND_ATTEMPTS,
        &SIERRA_COMPILER_LABELED_REMOTE_RESPONSE_TIMES_SECS,
        &SIERRA_COMPILER_LABELED_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES_SECS,
    ),
    LocalServerMetrics::new(
        &SIERRA_COMPILER_LOCAL_MSGS_RECEIVED,
        &SIERRA_COMPILER_LOCAL_MSGS_PROCESSED,
        &SIERRA_COMPILER_LOCAL_QUEUE_DEPTH,
        &SIERRA_COMPILER_LABELED_PROCESSING_TIMES_SECS,
        &SIERRA_COMPILER_LABELED_QUEUEING_TIMES_SECS,
    ),
    RemoteServerMetrics::new(
        &SIERRA_COMPILER_REMOTE_MSGS_RECEIVED,
        &SIERRA_COMPILER_REMOTE_VALID_MSGS_RECEIVED,
        &SIERRA_COMPILER_REMOTE_MSGS_PROCESSED,
        &SIERRA_COMPILER_REMOTE_NUMBER_OF_CONNECTIONS,
    ),
);
