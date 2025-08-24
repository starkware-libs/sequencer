use apollo_infra::metrics::{
    InfraMetrics,
    LocalClientMetrics,
    LocalServerMetrics,
    RemoteClientMetrics,
    RemoteServerMetrics,
    MEMPOOL_P2P_LOCAL_MSGS_PROCESSED,
    MEMPOOL_P2P_LOCAL_MSGS_RECEIVED,
    MEMPOOL_P2P_LOCAL_QUEUE_DEPTH,
    MEMPOOL_P2P_REMOTE_CLIENT_SEND_ATTEMPTS,
    MEMPOOL_P2P_REMOTE_MSGS_PROCESSED,
    MEMPOOL_P2P_REMOTE_MSGS_RECEIVED,
    MEMPOOL_P2P_REMOTE_NUMBER_OF_CONNECTIONS,
    MEMPOOL_P2P_REMOTE_VALID_MSGS_RECEIVED,
};
use apollo_mempool_p2p_types::communication::MEMPOOL_P2P_PROPAGATOR_REQUEST_LABELS;
use apollo_metrics::define_metrics;

define_metrics!(
    MempoolP2p => {
        // Gauges
        MetricGauge { MEMPOOL_P2P_NUM_CONNECTED_PEERS, "apollo_mempool_p2p_num_connected_peers", "The number of connected peers to the mempool p2p component" },
        MetricGauge { MEMPOOL_P2P_NUM_BLACKLISTED_PEERS, "apollo_mempool_p2p_num_blacklisted_peers", "The number of currently blacklisted peers by the mempool p2p component" },
        // Counters
        MetricCounter { MEMPOOL_P2P_NUM_SENT_MESSAGES, "apollo_mempool_p2p_num_sent_messages", "The number of messages sent by the mempool p2p component", init = 0 },
        MetricCounter { MEMPOOL_P2P_NUM_RECEIVED_MESSAGES, "apollo_mempool_p2p_num_received_messages", "The number of messages received by the mempool p2p component", init = 0 },
        // Histogram
        MetricHistogram { MEMPOOL_P2P_BROADCASTED_BATCH_SIZE, "apollo_mempool_p2p_broadcasted_transaction_batch_size", "The number of transactions in batches broadcast by the mempool p2p component" }
    },
    Infra => {
        // MempoolP2p request labels
        LabeledMetricHistogram { MEMPOOL_P2P_LABELED_PROCESSING_TIMES_SECS, "mempool_p2p_labeled_processing_times_secs", "Request processing times of the mempool p2p, per label (secs)", labels = MEMPOOL_P2P_PROPAGATOR_REQUEST_LABELS },
        LabeledMetricHistogram { MEMPOOL_P2P_LABELED_QUEUEING_TIMES_SECS, "mempool_p2p_labeled_queueing_times_secs", "Request queueing times of the mempool p2p, per label (secs)", labels = MEMPOOL_P2P_PROPAGATOR_REQUEST_LABELS },
        LabeledMetricHistogram { MEMPOOL_P2P_LABELED_LOCAL_RESPONSE_TIMES_SECS, "mempool_p2p_labeled_local_response_times_secs", "Request local response times of the mempool p2p, per label (secs)", labels = MEMPOOL_P2P_PROPAGATOR_REQUEST_LABELS },
        LabeledMetricHistogram { MEMPOOL_P2P_LABELED_REMOTE_RESPONSE_TIMES_SECS, "mempool_p2p_labeled_remote_response_times_secs", "Request remote response times of the mempool p2p, per label (secs)", labels = MEMPOOL_P2P_PROPAGATOR_REQUEST_LABELS },
        LabeledMetricHistogram { MEMPOOL_P2P_LABELED_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES_SECS, "mempool_p2p_labeled_remote_client_communication_failure_times_secs", "Request communication failure times of the mempool p2p, per label (secs)", labels = MEMPOOL_P2P_PROPAGATOR_REQUEST_LABELS },
    },
);

<<<<<<< HEAD
pub const _MEMPOOL_P2P_INFRA_METRICS: InfraMetrics = InfraMetrics::new(
=======
pub const MEMPOOL_P2P_INFRA_METRICS: InfraMetrics = InfraMetrics::new(
>>>>>>> 78117f681 (apollo_dashboard: use metrics structs to construct infra rows)
    LocalClientMetrics::new(&MEMPOOL_P2P_LABELED_LOCAL_RESPONSE_TIMES_SECS),
    RemoteClientMetrics::new(
        &MEMPOOL_P2P_REMOTE_CLIENT_SEND_ATTEMPTS,
        &MEMPOOL_P2P_LABELED_REMOTE_RESPONSE_TIMES_SECS,
        &MEMPOOL_P2P_LABELED_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES_SECS,
    ),
    LocalServerMetrics::new(
        &MEMPOOL_P2P_LOCAL_MSGS_RECEIVED,
        &MEMPOOL_P2P_LOCAL_MSGS_PROCESSED,
        &MEMPOOL_P2P_LOCAL_QUEUE_DEPTH,
        &MEMPOOL_P2P_LABELED_PROCESSING_TIMES_SECS,
        &MEMPOOL_P2P_LABELED_QUEUEING_TIMES_SECS,
    ),
    RemoteServerMetrics::new(
        &MEMPOOL_P2P_REMOTE_MSGS_RECEIVED,
        &MEMPOOL_P2P_REMOTE_VALID_MSGS_RECEIVED,
        &MEMPOOL_P2P_REMOTE_MSGS_PROCESSED,
        &MEMPOOL_P2P_REMOTE_NUMBER_OF_CONNECTIONS,
    ),
);
