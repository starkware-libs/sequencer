use apollo_config_manager_types::communication::CONFIG_MANAGER_REQUEST_LABELS;
use apollo_infra::metrics::{
    InfraMetrics,
    LocalClientMetrics,
    LocalServerMetrics,
    RemoteClientMetrics,
    RemoteServerMetrics,
    CONFIG_MANAGER_LOCAL_MSGS_PROCESSED,
    CONFIG_MANAGER_LOCAL_MSGS_RECEIVED,
    CONFIG_MANAGER_LOCAL_QUEUE_DEPTH,
    CONFIG_MANAGER_REMOTE_CLIENT_SEND_ATTEMPTS,
};
use apollo_metrics::define_metrics;

define_metrics!(
    Infra => {
        LabeledMetricHistogram {
            CONFIG_MANAGER_LABELED_PROCESSING_TIMES_SECS,
            "config_manager_labeled_processing_times_secs",
            "Request processing times of the config manager, per label (secs)",
            labels = CONFIG_MANAGER_REQUEST_LABELS
        },
        LabeledMetricHistogram {
            CONFIG_MANAGER_LABELED_QUEUEING_TIMES_SECS,
            "config_manager_labeled_queueing_times_secs",
            "Request queueing times of the config manager, per label (secs)",
            labels = CONFIG_MANAGER_REQUEST_LABELS
        },
        LabeledMetricHistogram {
            CONFIG_MANAGER_LABELED_LOCAL_RESPONSE_TIMES_SECS,
            "config_manager_labeled_local_response_times_secs",
            "Request local response times of the config manager, per label (secs)",
            labels = CONFIG_MANAGER_REQUEST_LABELS
        },
        LabeledMetricHistogram {
            CONFIG_MANAGER_LABELED_REMOTE_RESPONSE_TIMES_SECS,
            "config_manager_labeled_remote_response_times_secs",
            "Request remote response times of the config manager, per label (secs)",
            labels = CONFIG_MANAGER_REQUEST_LABELS
        },
        LabeledMetricHistogram {
            CONFIG_MANAGER_LABELED_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES_SECS,
            "config_manager_labeled_remote_client_communication_failure_times_secs",
            "Request communication failure times of the config manager, per label (secs)",
            labels = CONFIG_MANAGER_REQUEST_LABELS
        },
    },
);

pub const CONFIG_MANAGER_INFRA_METRICS: InfraMetrics = InfraMetrics::new(
    LocalClientMetrics::new(&CONFIG_MANAGER_LABELED_LOCAL_RESPONSE_TIMES_SECS),
    RemoteClientMetrics::new(
        &CONFIG_MANAGER_REMOTE_CLIENT_SEND_ATTEMPTS,
        &CONFIG_MANAGER_LABELED_REMOTE_RESPONSE_TIMES_SECS,
        &CONFIG_MANAGER_LABELED_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES_SECS,
    ),
    LocalServerMetrics::new(
        &CONFIG_MANAGER_LOCAL_MSGS_RECEIVED,
        &CONFIG_MANAGER_LOCAL_MSGS_PROCESSED,
        &CONFIG_MANAGER_LOCAL_QUEUE_DEPTH,
        &CONFIG_MANAGER_LABELED_PROCESSING_TIMES_SECS,
        &CONFIG_MANAGER_LABELED_QUEUEING_TIMES_SECS,
    ),
    RemoteServerMetrics::new(
        // ConfigManager doesn't support remote mode, so these are unused placeholders
        &CONFIG_MANAGER_LOCAL_MSGS_RECEIVED,
        &CONFIG_MANAGER_LOCAL_MSGS_RECEIVED,
        &CONFIG_MANAGER_LOCAL_MSGS_PROCESSED,
        &CONFIG_MANAGER_LOCAL_QUEUE_DEPTH,
    ),
);
