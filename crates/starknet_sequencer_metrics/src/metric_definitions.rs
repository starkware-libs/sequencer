use crate::metrics::{LabeledMetricCounter, MetricCounter, MetricGauge, MetricScope};

/// Macro to define all metric constants for specified scopes and store them in a collection.
/// This generates:
/// - Individual metric constant according to type: `MetricCounter`or `MetricGauge` or
///   `LabeledMetricCounter`.
/// - A const array `ALL_METRICS` containing all $keys of all the metrics constants.
#[macro_export]
macro_rules! define_metrics {
    (
        $(
            $scope:ident => {
                $(
                    $type:ty { $name:ident, $key:expr, $desc:expr $(, $init:expr)? }
                ),*
                $(,)?
            }
        ),*
        $(,)?
    ) => {
        $(
            $(
                pub const $name: $type = <$type>::new(
                    MetricScope::$scope,
                    $key,
                    $desc
                    $(, $init)?
                );
            )*
        )*
        // TODO(Lev): change macro to output this only for cfg[(test,testing)
        $(
            $crate::paste::paste! {
                pub const [<$scope:snake:upper _ALL_METRICS>]: &[&'static str] = &[
                    $(
                        $key,
                    )*
                ];
            }
        )*
    };
}

define_metrics!(
    HttpServer => {
        MetricCounter { ADDED_TRANSACTIONS_TOTAL, "http_server_added_transactions_total", "Total number of transactions added", 0 },
        MetricCounter { ADDED_TRANSACTIONS_SUCCESS, "http_server_added_transactions_success", "Number of successfully added transactions", 0 },
        MetricCounter { ADDED_TRANSACTIONS_FAILURE, "http_server_added_transactions_failure", "Number of faulty added transactions", 0 },
    },
);

define_metrics!(
    Infra => {
        // Local server counters
        MetricCounter { BATCHER_LOCAL_MSGS_RECEIVED, "batcher_local_msgs_received", "Counter of messages received by batcher local server", 0 },
        MetricCounter { BATCHER_LOCAL_MSGS_PROCESSED, "batcher_local_msgs_processed", "Counter of messages processed by batcher local server", 0 },
        MetricCounter { CLASS_MANAGER_LOCAL_MSGS_RECEIVED, "class_manager_local_msgs_received", "Counter of messages received by class manager local server", 0 },
        MetricCounter { CLASS_MANAGER_LOCAL_MSGS_PROCESSED, "class_manager_local_msgs_processed", "Counter of messages processed by class manager local server", 0 },
        MetricCounter { GATEWAY_LOCAL_MSGS_RECEIVED, "gateway_local_msgs_received", "Counter of messages received by gateway local server", 0 },
        MetricCounter { GATEWAY_LOCAL_MSGS_PROCESSED, "gateway_local_msgs_processed", "Counter of messages processed by gateway local server", 0 },
        MetricCounter { L1_PROVIDER_LOCAL_MSGS_RECEIVED, "l1_provider_local_msgs_received", "Counter of messages received by L1 provider local server", 0 },
        MetricCounter { L1_PROVIDER_LOCAL_MSGS_PROCESSED, "l1_provider_local_msgs_processed", "Counter of messages processed by L1 provider local server", 0 },
        MetricCounter { MEMPOOL_LOCAL_MSGS_RECEIVED, "mempool_local_msgs_received", "Counter of messages received by mempool local server", 0 },
        MetricCounter { MEMPOOL_LOCAL_MSGS_PROCESSED, "mempool_local_msgs_processed", "Counter of messages processed by mempool local server", 0 },
        MetricCounter { MEMPOOL_P2P_LOCAL_MSGS_RECEIVED, "mempool_p2p_propagator_local_msgs_received", "Counter of messages received by mempool p2p local server", 0 },
        MetricCounter { MEMPOOL_P2P_LOCAL_MSGS_PROCESSED, "mempool_p2p_propagator_local_msgs_processed", "Counter of messages processed by mempool p2p local server", 0 },
        MetricCounter { SIERRA_COMPILER_LOCAL_MSGS_RECEIVED, "sierra_compiler_local_msgs_received", "Counter of messages received by sierra compiler local server", 0 },
        MetricCounter { SIERRA_COMPILER_LOCAL_MSGS_PROCESSED, "sierra_compiler_local_msgs_processed", "Counter of messages processed by sierra compiler local server", 0 },
        MetricCounter { STATE_SYNC_LOCAL_MSGS_RECEIVED, "state_sync_local_msgs_received", "Counter of messages received by state sync local server", 0 },
        MetricCounter { STATE_SYNC_LOCAL_MSGS_PROCESSED, "state_sync_local_msgs_processed", "Counter of messages processed by state sync local server", 0 },
        // Remote server counters
        MetricCounter { BATCHER_REMOTE_MSGS_RECEIVED, "batcher_remote_msgs_received", "Counter of messages received by batcher remote server", 0 },
        MetricCounter { BATCHER_REMOTE_VALID_MSGS_RECEIVED, "batcher_remote_valid_msgs_received", "Counter of valid messages received by batcher remote server", 0 },
        MetricCounter { BATCHER_REMOTE_MSGS_PROCESSED, "batcher_remote_msgs_processed", "Counter of messages processed by batcher remote server", 0 },
        MetricCounter { CLASS_MANAGER_REMOTE_MSGS_RECEIVED, "class_manager_remote_msgs_received", "Counter of messages received by class manager remote server", 0 },
        MetricCounter { CLASS_MANAGER_REMOTE_VALID_MSGS_RECEIVED, "class_manager_remote_valid_msgs_received", "Counter of valid messages received by class manager remote server", 0 },
        MetricCounter { CLASS_MANAGER_REMOTE_MSGS_PROCESSED, "class_manager_remote_msgs_processed", "Counter of messages processed by class manager remote server", 0 },
        MetricCounter { GATEWAY_REMOTE_MSGS_RECEIVED, "gateway_remote_msgs_received", "Counter of messages received by gateway remote server", 0 },
        MetricCounter { GATEWAY_REMOTE_VALID_MSGS_RECEIVED, "gateway_remote_valid_msgs_received", "Counter of valid messages received by gateway remote server", 0 },
        MetricCounter { GATEWAY_REMOTE_MSGS_PROCESSED, "gateway_remote_msgs_processed", "Counter of messages processed by gateway remote server", 0 },
        MetricCounter { L1_PROVIDER_REMOTE_MSGS_RECEIVED, "l1_provider_remote_msgs_received", "Counter of messages received by L1 provider remote server", 0 },
        MetricCounter { L1_PROVIDER_REMOTE_VALID_MSGS_RECEIVED, "l1_provider_remote_valid_msgs_received", "Counter of valid messages received by L1 provider remote server", 0 },
        MetricCounter { L1_PROVIDER_REMOTE_MSGS_PROCESSED, "l1_provider_remote_msgs_processed", "Counter of messages processed by L1 provider remote server", 0 },
        MetricCounter { MEMPOOL_REMOTE_MSGS_RECEIVED, "mempool_remote_msgs_received", "Counter of messages received by mempool remote server", 0 },
        MetricCounter { MEMPOOL_REMOTE_VALID_MSGS_RECEIVED, "mempool_remote_valid_msgs_received", "Counter of valid messages received by mempool remote server", 0 },
        MetricCounter { MEMPOOL_REMOTE_MSGS_PROCESSED, "mempool_remote_msgs_processed", "Counter of messages processed by mempool remote server", 0 },
        MetricCounter { MEMPOOL_P2P_REMOTE_MSGS_RECEIVED, "mempool_p2p_propagator_remote_msgs_received", "Counter of messages received by mempool p2p remote server", 0 },
        MetricCounter { MEMPOOL_P2P_REMOTE_VALID_MSGS_RECEIVED, "mempool_p2p_propagator_remote_valid_msgs_received", "Counter of valid messages received by mempool p2p remote server", 0 },
        MetricCounter { MEMPOOL_P2P_REMOTE_MSGS_PROCESSED, "mempool_p2p_propagator_remote_msgs_processed", "Counter of messages processed by mempool p2p remote server", 0 },
        MetricCounter { STATE_SYNC_REMOTE_MSGS_RECEIVED, "state_sync_remote_msgs_received", "Counter of messages received by state sync remote server", 0 },
        MetricCounter { STATE_SYNC_REMOTE_VALID_MSGS_RECEIVED, "state_sync_remote_valid_msgs_received", "Counter of valid messages received by state sync remote server", 0 },
        MetricCounter { STATE_SYNC_REMOTE_MSGS_PROCESSED, "state_sync_remote_msgs_processed", "Counter of messages processed by state sync remote server", 0 },
        // Local server queue depths
        MetricGauge { BATCHER_LOCAL_QUEUE_DEPTH, "batcher_local_queue_depth", "The depth of the batcher's local message queue" },
        MetricGauge { CLASS_MANAGER_LOCAL_QUEUE_DEPTH, "class_manager_local_queue_depth", "The depth of the class manager's local message queue" },
        MetricGauge { GATEWAY_LOCAL_QUEUE_DEPTH, "gateway_local_queue_depth", "The depth of the gateway's local message queue" },
        MetricGauge { L1_PROVIDER_LOCAL_QUEUE_DEPTH, "l1_provider_local_queue_depth", "The depth of the L1 provider's local message queue" },
        MetricGauge { MEMPOOL_LOCAL_QUEUE_DEPTH, "mempool_local_queue_depth", "The depth of the mempool's local message queue" },
        MetricGauge { MEMPOOL_P2P_LOCAL_QUEUE_DEPTH, "mempool_p2p_propagator_local_queue_depth", "The depth of the mempool p2p's local message queue" },
        MetricGauge { SIERRA_COMPILER_LOCAL_QUEUE_DEPTH, "sierra_compiler_local_queue_depth", "The depth of the sierra compiler's local message queue" },
        MetricGauge { STATE_SYNC_LOCAL_QUEUE_DEPTH, "state_sync_local_queue_depth", "The depth of the state sync's local message queue" },
    },
);

define_metrics!(
    Mempool => {
        MetricCounter { MEMPOOL_TRANSACTIONS_COMMITTED, "mempool_txs_committed", "The number of transactions that were committed to block", 0 },
        LabeledMetricCounter { MEMPOOL_TRANSACTIONS_RECEIVED, "mempool_transactions_received", "Counter of transactions received by the mempool", 0 },
        LabeledMetricCounter { MEMPOOL_TRANSACTIONS_DROPPED, "mempool_transactions_dropped", "Counter of transactions dropped from the mempool", 0 },
    },
);

define_metrics!(
    Network => {
        // Gauges
        MetricGauge { MEMPOOL_P2P_NUM_CONNECTED_PEERS, "apollo_mempool_p2p_num_connected_peers", "The number of connected peers to the mempool p2p component" },
        MetricGauge { CONSENSUS_NUM_CONNECTED_PEERS, "apollo_consensus_num_connected_peers", "The number of connected peers to the consensus p2p component" },
        MetricGauge { STATE_SYNC_P2P_NUM_CONNECTED_PEERS, "apollo_sync_num_connected_peers", "The number of connected peers to the state sync p2p component" },
        MetricGauge { STATE_SYNC_P2P_NUM_ACTIVE_INBOUND_SESSIONS, "apollo_sync_num_active_inbound_sessions", "The number of inbound sessions to the state sync p2p component" },
        MetricGauge { STATE_SYNC_P2P_NUM_ACTIVE_OUTBOUND_SESSIONS, "apollo_sync_num_active_outbound_sessions", "The number of outbound sessions to the state sync p2p component" },
        // Counters
        MetricCounter { MEMPOOL_P2P_NUM_SENT_MESSAGES, "apollo_mempool_num_sent_messages", "The number of messages sent by the mempool p2p component", 0 },
        MetricCounter { MEMPOOL_P2P_NUM_RECEIVED_MESSAGES, "apollo_mempool_num_received_messages", "The number of messages received by the mempool p2p component", 0 },
        MetricCounter { CONSENSUS_NUM_SENT_MESSAGES, "apollo_consensus_num_sent_messages", "The number of messages sent by the consensus p2p component", 0 },
        MetricCounter { CONSENSUS_NUM_RECEIVED_MESSAGES, "apollo_consensus_num_received_messages", "The number of messages received by the consensus p2p component", 0 },
    },
);

define_metrics!(
    PapyrusSync => {
        // Gauges
        MetricGauge { SYNC_HEADER_MARKER, "apollo_sync_header_marker", "The first block number for which sync does not have a header" },
        MetricGauge { SYNC_BODY_MARKER, "apollo_sync_body_marker", "The first block number for which sync does not have a body" },
        MetricGauge { SYNC_STATE_MARKER, "apollo_sync_state_marker", "The first block number for which sync does not have a state body" },
        MetricGauge { SYNC_COMPILED_CLASS_MARKER, "apollo_sync_compiled_class_marker", "The first block number for which sync does not have all of the corresponding compiled classes" },
        MetricGauge { SYNC_CLASS_MANAGER_MARKER, "apollo_sync_class_manager_marker", "The first block number for which sync does not guarantee all of the corresponding classes are stored in the class manager component" },
        MetricGauge { SYNC_BASE_LAYER_MARKER, "apollo_sync_base_layer_marker", "The first block number for which sync does not guarantee L1 finality" },
        MetricGauge { SYNC_CENTRAL_BLOCK_MARKER, "apollo_sync_central_block_marker", "The first block number that doesn't exist yet" },
        MetricGauge { SYNC_HEADER_LATENCY_SEC, "apollo_sync_header_latency", "The latency, in seconds, between a block timestamp (as state in its header) and the time sync stores the header" },
        // Counters
        // TODO(shahak): add to metric's dashboard
        MetricCounter { SYNC_PROCESSED_TRANSACTIONS, "apollo_sync_processed_transactions", "The number of transactions processed by the sync component", 0 },
    },
);

define_metrics!(
    ClassManager => {
        LabeledMetricCounter { N_CLASSES, "class_manager_n_classes", "Number of classes, by label (regular, deprecated)", 0 },
    },
);
