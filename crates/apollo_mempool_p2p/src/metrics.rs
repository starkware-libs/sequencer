use apollo_infra::metrics::{
    InfraMetrics,
    LocalClientMetrics,
    LocalServerMetrics,
    RemoteClientMetrics,
    RemoteServerMetrics,
};
use apollo_mempool_p2p_types::communication::MEMPOOL_P2P_REQUEST_LABELS;
use apollo_metrics::{define_infra_metrics, define_metrics};
use apollo_network::metrics::{EVENT_TYPE_LABELS, NETWORK_BROADCAST_DROP_LABELS};

define_infra_metrics!(mempool_p2p);

define_metrics!(
    MempoolP2p => {
        // Gauges
        MetricGauge { MEMPOOL_P2P_NUM_CONNECTED_PEERS, "apollo_mempool_p2p_num_connected_peers", "The number of connected peers to the mempool p2p component" },
        MetricGauge { MEMPOOL_P2P_NUM_BLACKLISTED_PEERS, "apollo_mempool_p2p_num_blacklisted_peers", "The number of currently blacklisted peers by the mempool p2p component" },
        // Counters
        MetricCounter { MEMPOOL_P2P_NUM_SENT_MESSAGES, "apollo_mempool_p2p_num_sent_messages", "The number of messages sent by the mempool p2p component", init = 0 },
        MetricHistogram { MEMPOOL_P2P_SENT_MESSAGE_SIZE, "apollo_mempool_p2p_sent_message_size", "The size in MB of messages sent by the mempool p2p component" },
        MetricCounter { MEMPOOL_P2P_NUM_RECEIVED_MESSAGES, "apollo_mempool_p2p_num_received_messages", "The number of messages received by the mempool p2p component", init = 0 },
        MetricHistogram { MEMPOOL_P2P_RECEIVED_MESSAGE_SIZE, "apollo_mempool_p2p_received_message_size", "The size in MB of messages received by the mempool p2p component" },
        LabeledMetricCounter { MEMPOOL_P2P_NUM_DROPPED_MESSAGES, "apollo_mempool_p2p_num_dropped_messages", "The number of messages dropped by the mempool p2p component", init = 0, labels = NETWORK_BROADCAST_DROP_LABELS },
        MetricHistogram { MEMPOOL_P2P_DROPPED_MESSAGE_SIZE, "apollo_mempool_p2p_dropped_message_size", "The size in MB of messages dropped by the mempool p2p component" },
        // Histogram
        MetricHistogram { MEMPOOL_P2P_BROADCASTED_BATCH_SIZE, "apollo_mempool_p2p_broadcasted_transaction_batch_size", "The number of transactions in batches broadcast by the mempool p2p component" },
        MetricHistogram { MEMPOOL_P2P_PING_LATENCY, "apollo_mempool_p2p_ping_latency_seconds", "The ping latency in seconds for the mempool p2p component" },
        // Network events
        LabeledMetricCounter { MEMPOOL_P2P_NETWORK_EVENTS, "apollo_mempool_p2p_network_events", "Network events counter by event type for mempool p2p", init = 0, labels = EVENT_TYPE_LABELS }
    },
);
