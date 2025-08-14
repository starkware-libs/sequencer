use apollo_metrics::define_metrics;
use apollo_state_sync_types::communication::STATE_SYNC_REQUEST_LABELS;
use apollo_storage::body::BodyStorageReader;
use apollo_storage::class_manager::ClassManagerStorageReader;
use apollo_storage::compiled_class::CasmStorageReader;
use apollo_storage::db::TransactionKind;
use apollo_storage::header::HeaderStorageReader;
use apollo_storage::state::StateStorageReader;
use apollo_storage::StorageTxn;
use starknet_api::block::BlockNumber;

define_metrics!(
    StateSync => {
        // Central metrics.
        MetricGauge { CENTRAL_SYNC_BASE_LAYER_MARKER, "apollo_central_sync_base_layer_marker", "The first block number for which the central sync component does not guarantee L1 finality" },
        MetricGauge { CENTRAL_SYNC_CENTRAL_BLOCK_MARKER, "apollo_central_sync_central_block_marker", "The first block number that doesn't exist yet" },
        MetricCounter { CENTRAL_SYNC_FORKS_FROM_FEEDER, "apollo_central_sync_forks_from_central", "The number of times central has diverged from the sync's storage", init = 0 },
        // P2p metrics.
        MetricGauge { P2P_SYNC_NUM_CONNECTED_PEERS, "apollo_p2p_sync_num_connected_peers", "The number of connected peers to the p2p sync component" },
        MetricGauge { P2P_SYNC_NUM_ACTIVE_INBOUND_SESSIONS, "apollo_p2p_sync_num_active_inbound_sessions", "The number of inbound sessions to the p2p sync component" },
        MetricGauge { P2P_SYNC_NUM_ACTIVE_OUTBOUND_SESSIONS, "apollo_p2p_sync_num_active_outbound_sessions", "The number of outbound sessions to the p2p sync component" },
        MetricGauge { P2P_SYNC_NUM_BLACKLISTED_PEERS, "apollo_p2p_sync_num_blacklisted_peers", "The number of currently blacklisted peers by the p2p sync component" },
        // Metrics common to central and p2p.
        MetricGauge { STATE_SYNC_HEADER_MARKER, "apollo_state_sync_header_marker", "The first block number for which the state sync component does not have a header" },
        MetricGauge { STATE_SYNC_BODY_MARKER, "apollo_state_sync_body_marker", "The first block number for which the state sync component does not have a body" },
        MetricGauge { STATE_SYNC_STATE_MARKER, "apollo_state_sync_state_marker", "The first block number for which the state sync component does not have a state body" },
        MetricGauge { STATE_SYNC_COMPILED_CLASS_MARKER, "apollo_state_sync_compiled_class_marker", "The first block number for which the state sync component does not have all of the corresponding compiled classes" },
        MetricGauge { STATE_SYNC_CLASS_MANAGER_MARKER, "apollo_state_sync_class_manager_marker", "The first block number for which the state sync component does not guarantee all of the corresponding classes are stored in the class manager component" },
        MetricGauge { STATE_SYNC_HEADER_LATENCY_SEC, "apollo_state_sync_header_latency", "The latency, in seconds, between a block timestamp (as state in its header) and the time the state sync component stores the header" },
        MetricCounter { STATE_SYNC_PROCESSED_TRANSACTIONS, "apollo_state_sync_processed_transactions", "The number of transactions processed by the state sync component", init = 0 },
        MetricCounter { STATE_SYNC_REVERTED_TRANSACTIONS, "apollo_state_sync_reverted_transactions", "The number of transactions reverted by the state sync component", init = 0 },
    },
    Infra => {
        LabeledMetricHistogram { STATE_SYNC_LABELED_PROCESSING_TIMES_SECS, "state_sync_labeled_processing_times_secs", "Request processing times of the state sync, per label (secs)", labels = STATE_SYNC_REQUEST_LABELS },
    },
);

pub fn register_metrics<Mode: TransactionKind>(txn: &StorageTxn<'_, Mode>) {
    STATE_SYNC_HEADER_MARKER.register();
    STATE_SYNC_BODY_MARKER.register();
    STATE_SYNC_STATE_MARKER.register();
    STATE_SYNC_CLASS_MANAGER_MARKER.register();
    STATE_SYNC_COMPILED_CLASS_MARKER.register();
    STATE_SYNC_PROCESSED_TRANSACTIONS.register();
    STATE_SYNC_REVERTED_TRANSACTIONS.register();
    CENTRAL_SYNC_CENTRAL_BLOCK_MARKER.register();
    CENTRAL_SYNC_FORKS_FROM_FEEDER.register();
    update_marker_metrics(txn);
    reconstruct_processed_transactions_metric(txn);
}

pub fn update_marker_metrics<Mode: TransactionKind>(txn: &StorageTxn<'_, Mode>) {
    STATE_SYNC_HEADER_MARKER
        .set_lossy(txn.get_header_marker().expect("Should have a header marker").0);
    STATE_SYNC_BODY_MARKER.set_lossy(txn.get_body_marker().expect("Should have a body marker").0);
    STATE_SYNC_STATE_MARKER
        .set_lossy(txn.get_state_marker().expect("Should have a state marker").0);
    STATE_SYNC_CLASS_MANAGER_MARKER.set_lossy(
        txn.get_class_manager_block_marker().expect("Should have a class manager block marker").0,
    );
    STATE_SYNC_COMPILED_CLASS_MARKER
        .set_lossy(txn.get_compiled_class_marker().expect("Should have a compiled class marker").0);
}

fn reconstruct_processed_transactions_metric(txn: &StorageTxn<'_, impl TransactionKind>) {
    let block_marker = txn.get_body_marker().expect("Should have a body marker");

    for current_block_number in 0..block_marker.0 {
        let current_block_tx_count = txn
            .get_block_transactions_count(BlockNumber(current_block_number))
            .expect("Should have block transactions count")
            .expect("Missing block body with block number smaller than body marker");
        STATE_SYNC_PROCESSED_TRANSACTIONS
            .increment(current_block_tx_count.try_into().expect("Failed to convert usize to u64"));
    }
}
