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
    },
);
