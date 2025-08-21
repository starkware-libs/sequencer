use apollo_metrics::define_metrics;
use apollo_state_sync_types::communication::STATE_SYNC_REQUEST_LABELS;
use apollo_storage::body::BodyStorageReader;
use apollo_storage::class_manager::ClassManagerStorageReader;
use apollo_storage::compiled_class::CasmStorageReader;
use apollo_storage::db::TransactionKind;
use apollo_storage::header::HeaderStorageReader;
use apollo_storage::state::StateStorageReader;
use apollo_storage::{StorageReader, StorageTxn};
use starknet_api::block::BlockNumber;
use tracing::debug;

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
        LabeledMetricHistogram { STATE_SYNC_LABELED_QUEUEING_TIMES_SECS, "state_sync_labeled_queueing_times_secs", "Request queueing times of the state sync, per label (secs)", labels = STATE_SYNC_REQUEST_LABELS },
        LabeledMetricHistogram { STATE_SYNC_LABELED_LOCAL_RESPONSE_TIMES_SECS, "state_sync_labeled_local_response_times_secs", "Request local response times of the state sync, per label (secs)", labels = STATE_SYNC_REQUEST_LABELS },
        LabeledMetricHistogram { STATE_SYNC_LABELED_REMOTE_RESPONSE_TIMES_SECS, "state_sync_labeled_remote_response_times_secs", "Request remote response times of the state sync, per label (secs)", labels = STATE_SYNC_REQUEST_LABELS },
        LabeledMetricHistogram { STATE_SYNC_LABELED_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES_SECS, "state_sync_labeled_remote_client_communication_failure_times_secs", "Request communication failure times of the state sync, per label (secs)", labels = STATE_SYNC_REQUEST_LABELS },
    },
);

pub async fn register_metrics(storage_reader: StorageReader) {
    STATE_SYNC_HEADER_MARKER.register();
    STATE_SYNC_BODY_MARKER.register();
    STATE_SYNC_STATE_MARKER.register();
    STATE_SYNC_CLASS_MANAGER_MARKER.register();
    STATE_SYNC_COMPILED_CLASS_MARKER.register();
    STATE_SYNC_PROCESSED_TRANSACTIONS.register();
    STATE_SYNC_REVERTED_TRANSACTIONS.register();
    CENTRAL_SYNC_CENTRAL_BLOCK_MARKER.register();
    CENTRAL_SYNC_FORKS_FROM_FEEDER.register();
    let _ = tokio::task::spawn_blocking(move || {
        let txn = storage_reader.begin_ro_txn().unwrap();
        update_marker_metrics(&txn);
        reconstruct_processed_transactions_metric(&txn);
    })
    .await;
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

    debug!("Starting to count all transactions in the storage");
    // Early return if no blocks to process
    if block_marker.0 == 0 {
        return;
    }

    let mut total_transactions = 0;

    // Process all blocks efficiently
    for block_number in 0..block_marker.0 {
        if let Ok(Some(transaction_hashes)) =
            txn.get_block_transaction_hashes(BlockNumber(block_number))
        {
            total_transactions += transaction_hashes.len();
        }
    }

    debug!(
        "Finished counting all transactions in the storage. Incrementing {} metric with value: \
         {total_transactions}",
        STATE_SYNC_PROCESSED_TRANSACTIONS.get_name(),
    );
    // Set the metric once with the total count
    STATE_SYNC_PROCESSED_TRANSACTIONS
        .increment(total_transactions.try_into().expect("Failed to convert usize to u64"));
}
