use apollo_metrics::define_metrics;
use apollo_signature_manager_types::SIGNATURE_MANAGER_REQUEST_LABELS;

define_metrics! {
    SignatureManager => {},
    Infra => {
        MetricGauge { SIGNATURE_MANAGER_REMOTE_NUMBER_OF_CONNECTIONS, "signature_manager_remote_number_of_connections", "Number of connections to signature manager remote server" },
        // Define the labels for signature manager request metrics
        LabeledMetricHistogram {
            SIGNATURE_MANAGER_LABELED_PROCESSING_TIMES_SECS,
            "signature_manager_labeled_processing_times_secs",
            "Request processing times of the signature manager, per label (secs)",
            labels = SIGNATURE_MANAGER_REQUEST_LABELS
        },
        LabeledMetricHistogram {
            SIGNATURE_MANAGER_LABELED_QUEUEING_TIMES_SECS,
            "signature_manager_labeled_queueing_times_secs",
            "Request queueing times of the signature manager, per label (secs)",
            labels = SIGNATURE_MANAGER_REQUEST_LABELS
        },
        LabeledMetricHistogram {
            SIGNATURE_MANAGER_LABELED_LOCAL_RESPONSE_TIMES_SECS,
            "signature_manager_labeled_local_response_times_secs",
            "Request local response times of the signature manager, per label (secs)",
            labels = SIGNATURE_MANAGER_REQUEST_LABELS
        },
        LabeledMetricHistogram {
            SIGNATURE_MANAGER_LABELED_REMOTE_RESPONSE_TIMES_SECS,
            "signature_manager_labeled_remote_response_times_secs",
            "Request remote response times of the signature manager, per label (secs)",
            labels = SIGNATURE_MANAGER_REQUEST_LABELS
        },
        LabeledMetricHistogram {
            SIGNATURE_MANAGER_LABELED_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES_SECS,
            "signature_manager_labeled_remote_client_communication_failure_times_secs",
            "Request remote client communication failure times of the signature manager, per label (secs)",
            labels = SIGNATURE_MANAGER_REQUEST_LABELS
        },
    },
}
