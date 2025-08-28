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
        MetricCounter { MEMPOOL_P2P_NUM_DROPPED_MESSAGES, "apollo_mempool_p2p_num_dropped_messages", "The number of messages dropped by the mempool p2p component", init = 0 },
        // Histogram
        MetricHistogram { MEMPOOL_P2P_BROADCASTED_BATCH_SIZE, "apollo_mempool_p2p_broadcasted_transaction_batch_size", "The number of transactions in batches broadcast by the mempool p2p component" },

        // Event metrics
        MetricCounter { MEMPOOL_P2P_CONNECTIONS_ESTABLISHED, "apollo_mempool_p2p_connections_established", "The number of connections established by the mempool p2p component", init = 0 },
        MetricCounter { MEMPOOL_P2P_CONNECTIONS_CLOSED, "apollo_mempool_p2p_connections_closed", "The number of connections closed by the mempool p2p component", init = 0 },
        MetricCounter { MEMPOOL_P2P_DIAL_FAILURE, "apollo_mempool_p2p_dial_failure", "The number of dial failures by the mempool p2p component", init = 0 },
        MetricCounter { MEMPOOL_P2P_LISTEN_FAILURE, "apollo_mempool_p2p_listen_failure", "The number of listen failures by the mempool p2p component", init = 0 },
        MetricCounter { MEMPOOL_P2P_LISTEN_ERROR, "apollo_mempool_p2p_listen_error", "The number of listen errors by the mempool p2p component", init = 0 },
        MetricCounter { MEMPOOL_P2P_ADDRESS_CHANGE, "apollo_mempool_p2p_address_change", "The number of address changes by the mempool p2p component", init = 0 },
        MetricCounter { MEMPOOL_P2P_NEW_LISTENERS, "apollo_mempool_p2p_new_listeners", "The number of new listeners by the mempool p2p component", init = 0 },
        MetricCounter { MEMPOOL_P2P_NEW_LISTEN_ADDRS, "apollo_mempool_p2p_new_listen_addrs", "The number of new listen addresses by the mempool p2p component", init = 0 },
        MetricCounter { MEMPOOL_P2P_EXPIRED_LISTEN_ADDRS, "apollo_mempool_p2p_expired_listen_addrs", "The number of expired listen addresses by the mempool p2p component", init = 0 },
        MetricCounter { MEMPOOL_P2P_LISTENER_CLOSED, "apollo_mempool_p2p_listener_closed", "The number of listeners closed by the mempool p2p component", init = 0 },
        MetricCounter { MEMPOOL_P2P_NEW_EXTERNAL_ADDR_CANDIDATE, "apollo_mempool_p2p_new_external_addr_candidate", "The number of new external address candidates by the mempool p2p component", init = 0 },
        MetricCounter { MEMPOOL_P2P_EXTERNAL_ADDR_CONFIRMED, "apollo_mempool_p2p_external_addr_confirmed", "The number of external addresses confirmed by the mempool p2p component", init = 0 },
        MetricCounter { MEMPOOL_P2P_EXTERNAL_ADDR_EXPIRED, "apollo_mempool_p2p_external_addr_expired", "The number of external addresses expired by the mempool p2p component", init = 0 },
        MetricCounter { MEMPOOL_P2P_NEW_EXTERNAL_ADDR_OF_PEER, "apollo_mempool_p2p_new_external_addr_of_peer", "The number of new external addresses of peers by the mempool p2p component", init = 0 },
        MetricCounter { MEMPOOL_P2P_INBOUND_CONNECTIONS_HANDLED, "apollo_mempool_p2p_inbound_connections_handled", "The number of inbound connections handled by the mempool p2p component", init = 0 },
        MetricCounter { MEMPOOL_P2P_OUTBOUND_CONNECTIONS_HANDLED, "apollo_mempool_p2p_outbound_connections_handled", "The number of outbound connections handled by the mempool p2p component", init = 0 },
        MetricCounter { MEMPOOL_P2P_CONNECTION_HANDLER_EVENTS, "apollo_mempool_p2p_connection_handler_events", "The number of connection handler events by the mempool p2p component", init = 0 }
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

pub const MEMPOOL_P2P_INFRA_METRICS: InfraMetrics = InfraMetrics::new(
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
