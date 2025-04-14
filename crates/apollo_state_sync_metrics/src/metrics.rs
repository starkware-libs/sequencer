use apollo_metrics::define_metrics;
use apollo_metrics::metrics::{MetricCounter, MetricGauge};

define_metrics!(
    StateSync => {
        // Gauges
        MetricGauge { STATE_SYNC_P2P_NUM_CONNECTED_PEERS, "apollo_state_sync_p2p_num_connected_peers", "The number of connected peers to the state sync p2p component" },
        MetricGauge { STATE_SYNC_P2P_NUM_ACTIVE_INBOUND_SESSIONS, "apollo_state_sync_p2p_num_active_inbound_sessions", "The number of inbound sessions to the state sync p2p component" },
        MetricGauge { STATE_SYNC_P2P_NUM_ACTIVE_OUTBOUND_SESSIONS, "apollo_state_sync_p2p_num_active_outbound_sessions", "The number of outbound sessions to the state sync p2p component" },
        MetricGauge { STATE_SYNC_P2P_NUM_BLACKLISTED_PEERS, "apollo_state_sync_p2p_num_blacklisted_peers", "The number of currently blacklisted peers by the state sync p2p component" },
        MetricGauge { SYNC_HEADER_MARKER, "apollo_central_sync_header_marker", "The first block number for which sync does not have a header" },
        MetricGauge { SYNC_BODY_MARKER, "apollo_central_sync_body_marker", "The first block number for which sync does not have a body" },
        MetricGauge { SYNC_STATE_MARKER, "apollo_central_sync_state_marker", "The first block number for which sync does not have a state body" },
        MetricGauge { SYNC_COMPILED_CLASS_MARKER, "apollo_central_sync_compiled_class_marker", "The first block number for which sync does not have all of the corresponding compiled classes" },
        MetricGauge { SYNC_CLASS_MANAGER_MARKER, "apollo_central_sync_class_manager_marker", "The first block number for which sync does not guarantee all of the corresponding classes are stored in the class manager component" },
        MetricGauge { SYNC_BASE_LAYER_MARKER, "apollo_central_sync_base_layer_marker", "The first block number for which sync does not guarantee L1 finality" },
        MetricGauge { SYNC_CENTRAL_BLOCK_MARKER, "apollo_central_sync_central_block_marker", "The first block number that doesn't exist yet" },
        MetricGauge { SYNC_HEADER_LATENCY_SEC, "apollo_central_sync_header_latency", "The latency, in seconds, between a block timestamp (as state in its header) and the time sync stores the header" },
        // Counters
        // TODO(shahak): add to metric's dashboard
        MetricCounter { SYNC_PROCESSED_TRANSACTIONS, "apollo_central_sync_processed_transactions", "The number of transactions processed by the sync component", init = 0 },
        MetricCounter { SYNC_REVERTED_TRANSACTIONS, "apollo_central_sync_reverted_transactions", "The number of transactions reverted by the sync component", init = 0 },
    },
);
