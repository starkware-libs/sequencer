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
    MetricScope::Network => {
        { MEMPOOL_P2P_NUM_CONNECTED_PEERS, "apollo_mempool_p2p_num_connected_peers", "The number of connected peers to the mempool p2p component" },
        { MEMPOOL_P2P_NUM_ACTIVE_INBOUND_SESSIONS, "apollo_mempool_p2p_num_active_inbound_sessions", "The number of inbound sessions to the mempool p2p component" },
        { MEMPOOL_P2P_NUM_ACTIVE_OUTBOUND_SESSIONS, "apollo_mempool_p2p_num_active_outbound_sessions", "The number of outbound sessions to the mempool p2p component" },
        { CONSENSUS_NUM_CONNECTED_PEERS, "apollo_consensus_num_connected_peers", "The number of connected peers to the consensus p2p component" },
        { CONSENSUS_NUM_ACTIVE_INBOUND_SESSIONS, "apollo_consensus_num_active_inbound_sessions", "The number of inbound sessions to the consensus p2p component" },
        { CONSENSUS_NUM_ACTIVE_OUTBOUND_SESSIONS, "apollo_consensus_num_active_outbound_sessions", "The number of outbound sessions to the consensus p2p component" },
        { STATE_SYNC_NUM_CONNECTED_PEERS, "apollo_sync_num_connected_peers", "The number of connected peers to the state sync p2p component" },
        { STATE_SYNC_NUM_ACTIVE_INBOUND_SESSIONS, "apollo_sync_num_active_inbound_sessions", "The number of inbound sessions to the state sync p2p component" },
        { STATE_SYNC_NUM_ACTIVE_OUTBOUND_SESSIONS, "apollo_sync_num_active_outbound_sessions", "The number of outbound sessions to the state sync p2p component" },
    },
    MetricScope::PapyrusSync => {
        { PAPYRUS_HEADER_MARKER, "papyrus_header_marker", "The first block number for which the node does not have a header" },
        { PAPYRUS_BODY_MARKER, "papyrus_body_marker", "The first block number for which the node does not have a body" },
        { PAPYRUS_STATE_MARKER, "papyrus_state_marker", "The first block number for which the node does not have a state body" },
        { PAPYRUS_COMPILED_CLASS_MARKER, "papyrus_compiled_class_marker", "The first block number for which the node does not have all of the corresponding compiled classes" },
        { PAPYRUS_BASE_LAYER_MARKER, "papyrus_base_layer_marker", "The first block number for which the node does not guarantee L1 finality" },
        { PAPYRUS_CENTRAL_BLOCK_MARKER, "papyrus_central_block_marker", "The first block number that doesn't exist yet" },
        { PAPYRUS_HEADER_LATENCY_SEC, "papyrus_header_latency", "The latency, in seconds, between a block timestamp (as state in its header) and the time the node stores the header" },
    }
);

define_counter_metrics!(
    MetricScope::Batcher => {
        { PROPOSAL_STARTED, "batcher_proposal_started", "Counter of proposals started", 0 },
        { PROPOSAL_SUCCEEDED, "batcher_proposal_succeeded", "Counter of successful proposals", 0 },
        { PROPOSAL_FAILED, "batcher_proposal_failed", "Counter of failed proposals", 0 },
        { PROPOSAL_ABORTED, "batcher_proposal_aborted", "Counter of aborted proposals", 0 },
        { BATCHED_TRANSACTIONS, "batcher_batched_transactions", "Counter of batched transactions", 0 },
        { REJECTED_TRANSACTIONS, "batcher_rejected_transactions", "Counter of rejected transactions", 0 }
    },
    MetricScope::HttpServer => {
        { ADDED_TRANSACTIONS_TOTAL, "ADDED_TRANSACTIONS_TOTAL", "Total number of transactions added", 0 },
        { ADDED_TRANSACTIONS_SUCCESS, "ADDED_TRANSACTIONS_SUCCESS", "Number of successfully added transactions", 0 },
        { ADDED_TRANSACTIONS_FAILURE, "ADDED_TRANSACTIONS_FAILURE", "Number of faulty added transactions", 0 }
    },
);
