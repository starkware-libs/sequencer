use starknet_sequencer_metrics::define_metrics;
use starknet_sequencer_metrics::metrics::{MetricCounter, MetricGauge, MetricHistogram};

define_metrics!(
    MempoolP2p => {
        // Gauges
        MetricGauge { MEMPOOL_P2P_NUM_CONNECTED_PEERS, "apollo_mempool_p2p_num_connected_peers", "The number of connected peers to the mempool p2p component" },
        // Counters
        MetricCounter { MEMPOOL_P2P_NUM_SENT_MESSAGES, "apollo_mempool_p2p_num_sent_messages", "The number of messages sent by the mempool p2p component", init = 0 },
        MetricCounter { MEMPOOL_P2P_NUM_RECEIVED_MESSAGES, "apollo_mempool_p2p_num_received_messages", "The number of messages received by the mempool p2p component", init = 0 },
        // Histogram
        MetricHistogram { MEMPOOL_P2P_BROADCAST_BATCH_SIZE, "apollo_mempool_p2p_broadcast_transaction_batch_size", "The number of transactions in batches broadcast by the mempool p2p component" }
    },
);
