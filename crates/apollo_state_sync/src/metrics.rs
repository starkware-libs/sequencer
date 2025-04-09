use apollo_metrics::define_metrics;
use apollo_metrics::metrics::MetricGauge;

define_metrics!(
    StateSync => {
        // Gauges
        MetricGauge { P2P_SYNC_NUM_CONNECTED_PEERS, "apollo_p2p_sync_num_connected_peers", "The number of connected peers to the p2p sync component" },
        MetricGauge { P2P_SYNC_NUM_ACTIVE_INBOUND_SESSIONS, "apollo_p2p_sync_num_active_inbound_sessions", "The number of inbound sessions to the p2p sync component" },
        MetricGauge { P2P_SYNC_NUM_ACTIVE_OUTBOUND_SESSIONS, "apollo_p2p_sync_num_active_outbound_sessions", "The number of outbound sessions to the p2p sync component" },
        MetricGauge { P2P_SYNC_NUM_BLACKLISTED_PEERS, "apollo_p2p_sync_num_blacklisted_peers", "The number of currently blacklisted peers by the p2p sync component" },
    },
);
