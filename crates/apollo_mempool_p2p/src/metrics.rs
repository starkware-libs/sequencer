use apollo_metrics::define_metrics;

define_metrics!(
    MempoolP2p => {
        // Gauges
        MetricGauge { MEMPOOL_P2P_NUM_CONNECTED_PEERS, "apollo_mempool_p2p_num_connected_peers", "The number of connected peers to the mempool p2p component" },
        MetricGauge { MEMPOOL_P2P_NUM_BLACKLISTED_PEERS, "apollo_mempool_p2p_num_blacklisted_peers", "The number of currently blacklisted peers by the mempool p2p component" },
        // Counters
        MetricCounter { MEMPOOL_P2P_NUM_SENT_MESSAGES, "apollo_mempool_p2p_num_sent_messages", "The number of messages sent by the mempool p2p component", init = 0 },
        MetricCounter { MEMPOOL_P2P_NUM_RECEIVED_MESSAGES, "apollo_mempool_p2p_num_received_messages", "The number of messages received by the mempool p2p component", init = 0 },
        MetricCounter { MEMPOOL_P2P_NUM_INSUFFICIENT_PEERS_ERRORS, "apollo_mempool_p2p_num_insufficient_peers_errors", "The number of InsufficientPeers errors encountered while broadcasting messages", init = 0 },
        // Histogram
        MetricHistogram { MEMPOOL_P2P_BROADCASTED_BATCH_SIZE, "apollo_mempool_p2p_broadcasted_transaction_batch_size", "The number of transactions in batches broadcast by the mempool p2p component" }
    },
);
