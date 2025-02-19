use crate::metrics::{MetricCounter, MetricGauge, MetricScope};

#[cfg(test)]
#[path = "metric_definitions_test.rs"]
pub mod metric_definitions_test;

/// Macro to define `MetricCounter` constants for specified scopes and store them in a collection.
/// This generates:
/// - Individual `MetricCounter` constants (e.g., `PROPOSAL_STARTED`).
/// - A const array `ALL_METRIC_COUNTERS` containing all defined `MetricCounter` constants.
macro_rules! define_counter_metrics {
    (
        $(
            $scope:expr => {
                $(
                    { $name:ident, $key:expr, $desc:expr, $init:expr }
                ),*
                $(,)?
            }
        ),*
        $(,)?
    ) => {
        $(
            $(
                pub const $name: MetricCounter = MetricCounter::new(
                    $scope,
                    $key,
                    $desc,
                    $init
                );
            )*
        )*

        pub const ALL_METRIC_COUNTERS: &[MetricCounter] = &[
            $(
                $($name),*
            ),*
        ];
    };
}

/// Macro to define `MetricGauge` constants for specified scopes and store them in a collection.
/// This generates:
/// - Individual `MetricGauge` constants (e.g., `STORAGE_HEIGHT`).
/// - A `const` array `ALL_METRIC_GAUGES` containing all defined `MetricGauge` constants.
macro_rules! define_gauge_metrics {
    (
        $(
            $scope:expr => {
                $(
                    { $name:ident, $key:expr, $desc:expr }
                ),*
                $(,)?
            }
        ),*
        $(,)?
    ) => {
        $(
            $(
                pub const $name: MetricGauge = MetricGauge::new(
                    $scope,
                    $key,
                    $desc
                );
            )*
        )*

        pub const ALL_METRIC_GAUGES: &[MetricGauge] = &[
            $(
                $($name),*
            ),*
        ];
    };
}

define_gauge_metrics!(
    MetricScope::Batcher => {
        { STORAGE_HEIGHT, "batcher_storage_height", "The height of the batcher's storage" }
    },
    MetricScope::Infra => {
        { BATCHER_QUEUE_DEPTH, "batcher_queue_depth", "The depth of the batcher's message queue" },
        { CLASS_MANAGER_QUEUE_DEPTH, "class_manager_queue_depth", "The depth of the class manager's message queue" },
        { GATEWAY_QUEUE_DEPTH, "gateway_queue_depth", "The depth of the gateway's message queue" },
        { L1_PROVIDER_QUEUE_DEPTH, "l1_provider_queue_depth", "The depth of the L1 provider's message queue" },
        { MEMPOOL_QUEUE_DEPTH, "mempool_communication_wrapper_queue_depth", "The depth of the mempool's message queue" },
        { MEMPOOL_P2P_QUEUE_DEPTH, "mempool_p2p_propagator_queue_depth", "The depth of the mempool p2p's message queue" },
        { SIERRA_COMPILER_QUEUE_DEPTH, "sierra_compiler_queue_depth", "The depth of the sierra compiler's message queue" },
        { STATE_SYNC_QUEUE_DEPTH, "state_sync_queue_depth", "The depth of the state sync's message queue" },
    },
    MetricScope::Network => {
        { MEMPOOL_P2P_NUM_CONNECTED_PEERS, "apollo_mempool_p2p_num_connected_peers", "The number of connected peers to the mempool p2p component" },
        { CONSENSUS_NUM_CONNECTED_PEERS, "apollo_consensus_num_connected_peers", "The number of connected peers to the consensus p2p component" },
        { STATE_SYNC_P2P_NUM_CONNECTED_PEERS, "apollo_sync_num_connected_peers", "The number of connected peers to the state sync p2p component" },
        { STATE_SYNC_P2P_NUM_ACTIVE_INBOUND_SESSIONS, "apollo_sync_num_active_inbound_sessions", "The number of inbound sessions to the state sync p2p component" },
        { STATE_SYNC_P2P_NUM_ACTIVE_OUTBOUND_SESSIONS, "apollo_sync_num_active_outbound_sessions", "The number of outbound sessions to the state sync p2p component" },
    },
    MetricScope::PapyrusSync => {
        { SYNC_HEADER_MARKER, "apollo_sync_header_marker", "The first block number for which sync does not have a header" },
        { SYNC_BODY_MARKER, "apollo_sync_body_marker", "The first block number for which sync does not have a body" },
        { SYNC_STATE_MARKER, "apollo_sync_state_marker", "The first block number for which sync does not have a state body" },
        { SYNC_COMPILED_CLASS_MARKER, "apollo_sync_compiled_class_marker", "The first block number for which sync does not have all of the corresponding compiled classes" },
        { SYNC_BASE_LAYER_MARKER, "apollo_sync_base_layer_marker", "The first block number for which sync does not guarantee L1 finality" },
        { SYNC_CENTRAL_BLOCK_MARKER, "apollo_sync_central_block_marker", "The first block number that doesn't exist yet" },
        { SYNC_HEADER_LATENCY_SEC, "apollo_sync_header_latency", "The latency, in seconds, between a block timestamp (as state in its header) and the time sync stores the header" },
    }
);

define_counter_metrics!(
    MetricScope::Batcher => {
        { PROPOSAL_STARTED, "batcher_proposal_started", "Counter of proposals started", 0 },
        { PROPOSAL_SUCCEEDED, "batcher_proposal_succeeded", "Counter of successful proposals", 0 },
        { PROPOSAL_FAILED, "batcher_proposal_failed", "Counter of failed proposals", 0 },
        { PROPOSAL_ABORTED, "batcher_proposal_aborted", "Counter of aborted proposals", 0 },
        { BATCHED_TRANSACTIONS, "batcher_batched_transactions", "Counter of batched transactions across all forks", 0 },
        { REJECTED_TRANSACTIONS, "batcher_rejected_transactions", "Counter of rejected transactions", 0 }
    },
    MetricScope::HttpServer => {
        { ADDED_TRANSACTIONS_TOTAL, "ADDED_TRANSACTIONS_TOTAL", "Total number of transactions added", 0 },
        { ADDED_TRANSACTIONS_SUCCESS, "ADDED_TRANSACTIONS_SUCCESS", "Number of successfully added transactions", 0 },
        { ADDED_TRANSACTIONS_FAILURE, "ADDED_TRANSACTIONS_FAILURE", "Number of faulty added transactions", 0 }
    },
    MetricScope::Infra => {
        // TODO(Lev): rename local server counters to match the remote server counters including in description.
        // Local server counters
        { BATCHER_MSGS_RECEIVED, "batcher_msgs_received", "Counter of messages received by batcher component", 0 },
        { BATCHER_MSGS_PROCESSED, "batcher_msgs_processed", "Counter of messages  processed by batcher component", 0 },
        { CLASS_MANAGER_MSGS_RECEIVED, "class_manager_msgs_received", "Counter of messages received by class manager component", 0 },
        { CLASS_MANAGER_MSGS_PROCESSED, "class_manager_msgs_processed", "Counter of messages processed by class manager component", 0 },
        { GATEWAY_MSGS_RECEIVED, "gateway_msgs_received", "Counter of messages received by gateway component", 0 },
        { GATEWAY_MSGS_PROCESSED, "gateway_msgs_processed", "Counter of messages processed by gateway component", 0 },
        { L1_PROVIDER_MSGS_RECEIVED, "l1_provider_msgs_received", "Counter of messages received by L1 provider component", 0 },
        { L1_PROVIDER_MSGS_PROCESSED, "l1_provider_msgs_processed", "Counter of messages processed by L1 provider component", 0 },
        { MEMPOOL_MSGS_RECEIVED, "mempool_communication_wrapper_msgs_received", "Counter of messages received by mempool component", 0 },
        { MEMPOOL_MSGS_PROCESSED, "mempool_communication_wrapper_msgs_processed", "Counter of messages processed by mempool component", 0 },
        { MEMPOOL_P2P_MSGS_RECEIVED, "mempool_p2p_propagator_msgs_received", "Counter of messages received by mempool p2p component", 0 },
        { MEMPOOL_P2P_MSGS_PROCESSED, "mempool_p2p_propagator_msgs_processed", "Counter of messages processed by mempool p2p component", 0 },
        { SIERRA_COMPILER_MSGS_RECEIVED, "sierra_compiler_msgs_received", "Counter of messages received by sierra compiler component", 0 },
        { SIERRA_COMPILER_MSGS_PROCESSED, "sierra_compiler_msgs_processed", "Counter of messages processed by sierra compiler component", 0 },
        { STATE_SYNC_MSGS_RECEIVED, "state_sync_msgs_received", "Counter of messages received by state sync component", 0 },
        { STATE_SYNC_MSGS_PROCESSED, "state_sync_msgs_processed", "Counter of messages processed by state sync component", 0 },
        // Remote server counters
        { BATCHER_REMOTE_MSGS_RECEIVED, "batcher_remote_msgs_received", "Counter of messages received by batcher remote server", 0 },
        { BATCHER_REMOTE_VALID_MSGS_RECEIVED, "batcher_remote_valid_msgs_received", "Counter of valid messages received by batcher remote server", 0 },
        { BATCHER_REMOTE_MSGS_PROCESSED, "batcher_remote_msgs_processed", "Counter of messages  processed by batcher remote server", 0 },
        { CLASS_MANAGER_REMOTE_MSGS_RECEIVED, "class_manager_remote_msgs_received", "Counter of messages received by class manager remote server", 0 },
        { CLASS_MANAGER_REMOTE_VALID_MSGS_RECEIVED, "class_manager_remote_valid_msgs_received", "Counter of valid messages received by class manager remote server", 0 },
        { CLASS_MANAGER_REMOTE_MSGS_PROCESSED, "class_manager_remote_msgs_processed", "Counter of messages processed by class manager remote server", 0 },
        { GATEWAY_REMOTE_MSGS_RECEIVED, "gateway_remote_msgs_received", "Counter of messages received by gateway remote server", 0 },
        { GATEWAY_REMOTE_VALID_MSGS_RECEIVED, "gateway_remote_valid_msgs_received", "Counter of valid messages received by gateway remote server", 0 },
        { GATEWAY_REMOTE_MSGS_PROCESSED, "gateway_remote_msgs_processed", "Counter of messages processed by gateway remote server", 0 },
        { L1_PROVIDER_REMOTE_MSGS_RECEIVED, "l1_provider_remote_msgs_received", "Counter of messages received by L1 provider remote server", 0 },
        { L1_PROVIDER_REMOTE_VALID_MSGS_RECEIVED, "l1_provider_remote_valid_msgs_received", "Counter of valid messages received by L1 provider remote server", 0 },
        { L1_PROVIDER_REMOTE_MSGS_PROCESSED, "l1_provider_remote_msgs_processed", "Counter of messages processed by L1 provider remote server", 0 },
        { MEMPOOL_REMOTE_MSGS_RECEIVED, "mempool_remote_msgs_received", "Counter of messages received by mempool remote server", 0 },
        { MEMPOOL_REMOTE_VALID_MSGS_RECEIVED, "mempool_remote_valid_msgs_received", "Counter of valid messages received by mempool remote server", 0 },
        { MEMPOOL_REMOTE_MSGS_PROCESSED, "mempool_remote_msgs_processed", "Counter of messages processed by mempool remote server", 0 },
        { MEMPOOL_P2P_REMOTE_MSGS_RECEIVED, "mempool_p2p_propagator_remote_msgs_received", "Counter of messages received by mempool p2p remote server", 0 },
        { MEMPOOL_P2P_REMOTE_VALID_MSGS_RECEIVED, "mempool_p2p_propagator_remote_valid_msgs_received", "Counter of valid messages received by mempool p2p remote server", 0 },
        { MEMPOOL_P2P_REMOTE_MSGS_PROCESSED, "mempool_p2p_propagator_remote_msgs_processed", "Counter of messages processed by mempool p2p remote server", 0 },
        { STATE_SYNC_REMOTE_MSGS_RECEIVED, "state_sync_remote_msgs_received", "Counter of messages received by state sync remote server", 0 },
        { STATE_SYNC_REMOTE_VALID_MSGS_RECEIVED, "state_sync_remote_valid_msgs_received", "Counter of valid messages received by state sync remote server", 0 },
        { STATE_SYNC_REMOTE_MSGS_PROCESSED, "state_sync_remote_msgs_processed", "Counter of messages processed by state sync remote server", 0 },
    },
    MetricScope::Network => {
        { MEMPOOL_P2P_NUM_SENT_MESSAGES, "apollo_mempool_num_sent_messages", "The number of messages sent by the mempool p2p component", 0 },
        { MEMPOOL_P2P_NUM_RECEIVED_MESSAGES, "apollo_mempool_num_received_messages", "The number of messages received by the mempool p2p component", 0 },
        { CONSENSUS_NUM_SENT_MESSAGES, "apollo_consensus_num_sent_messages", "The number of messages sent by the consensus p2p component", 0 },
        { CONSENSUS_NUM_RECEIVED_MESSAGES, "apollo_consensus_num_received_messages", "The number of messages received by the consensus p2p component", 0 },
    },
);
