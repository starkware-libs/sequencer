use starknet_sequencer_metrics::define_metrics;
use starknet_sequencer_metrics::metrics::MetricGauge;

define_metrics!(
    StateSync => {
        // Gauges
        MetricGauge { STATE_SYNC_P2P_NUM_CONNECTED_PEERS, "apollo_central_sync_num_connected_peers", "The number of connected peers to the state sync p2p component" },
        MetricGauge { STATE_SYNC_P2P_NUM_ACTIVE_INBOUND_SESSIONS, "apollo_central_sync_num_active_inbound_sessions", "The number of inbound sessions to the state sync p2p component" },
        MetricGauge { STATE_SYNC_P2P_NUM_ACTIVE_OUTBOUND_SESSIONS, "apollo_central_sync_num_active_outbound_sessions", "The number of outbound sessions to the state sync p2p component" },
        MetricGauge { STATE_SYNC_P2P_NUM_BLACKLISTED_PEERS, "apollo_central_sync_num_blacklisted_peers", "The number of currently blacklisted peers by the state sync p2p component" },
    },
);
