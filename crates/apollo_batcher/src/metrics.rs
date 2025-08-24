use apollo_batcher_types::communication::BATCHER_REQUEST_LABELS;
use apollo_infra::metrics::{
    InfraMetrics,
    LocalClientMetrics,
    LocalServerMetrics,
    RemoteClientMetrics,
    RemoteServerMetrics,
    BATCHER_LOCAL_MSGS_PROCESSED,
    BATCHER_LOCAL_MSGS_RECEIVED,
    BATCHER_LOCAL_QUEUE_DEPTH,
    BATCHER_REMOTE_CLIENT_SEND_ATTEMPTS,
    BATCHER_REMOTE_MSGS_PROCESSED,
    BATCHER_REMOTE_MSGS_RECEIVED,
    BATCHER_REMOTE_NUMBER_OF_CONNECTIONS,
    BATCHER_REMOTE_VALID_MSGS_RECEIVED,
};
use apollo_metrics::define_metrics;
use blockifier::metrics::{
    CALLS_RUNNING_NATIVE,
    CLASS_CACHE_HITS,
    CLASS_CACHE_MISSES,
    NATIVE_CLASS_RETURNED,
    NATIVE_COMPILATION_ERROR,
    TOTAL_CALLS,
};
use starknet_api::block::BlockNumber;

define_metrics!(
    Batcher => {
        // Heights
        MetricGauge { STORAGE_HEIGHT, "batcher_storage_height", "The height of the batcher's storage" },
        MetricGauge { LAST_BATCHED_BLOCK, "batcher_last_batched_block", "The last block received by batching" },
        MetricGauge { LAST_SYNCED_BLOCK, "batcher_last_synced_block", "The last block received by syncing" },
        MetricGauge { LAST_PROPOSED_BLOCK, "batcher_last_proposed_block", "The last block proposed by this sequencer" },
        MetricCounter { REVERTED_BLOCKS, "batcher_reverted_blocks", "Counter of reverted blocks", init = 0 },
        // Proposals
        MetricCounter { PROPOSAL_STARTED, "batcher_proposal_started", "Counter of proposals started", init = 0 },
        MetricCounter { PROPOSAL_SUCCEEDED, "batcher_proposal_succeeded", "Counter of successful proposals", init = 0 },
        MetricCounter { PROPOSAL_FAILED, "batcher_proposal_failed", "Counter of failed proposals", init = 0 },
        MetricCounter { PROPOSAL_ABORTED, "batcher_proposal_aborted", "Counter of aborted proposals", init = 0 },
        // Transactions
        MetricCounter { BATCHED_TRANSACTIONS, "batcher_batched_transactions", "Counter of batched transactions across all forks", init = 0 },
        MetricCounter { REJECTED_TRANSACTIONS, "batcher_rejected_transactions", "Counter of rejected transactions", init = 0 },
        MetricCounter { REVERTED_TRANSACTIONS, "batcher_reverted_transactions", "Counter of reverted transactions across all forks", init = 0 },
        MetricCounter { SYNCED_TRANSACTIONS, "batcher_synced_transactions", "Counter of synced transactions", init = 0 },

        MetricCounter { FULL_BLOCKS, "batcher_full_blocks", "Counter of blocks closed on full capacity", init = 0 },
        MetricCounter { PRECONFIRMED_BLOCK_WRITTEN, "batcher_preconfirmed_block_written", "Counter of preconfirmed blocks written to storage", init = 0 },
    },
    Infra => {
        // Batcher request labels
        LabeledMetricHistogram { BATCHER_LABELED_PROCESSING_TIMES_SECS, "batcher_labeled_processing_times_secs", "Request processing times of the batcher, per label (secs)", labels = BATCHER_REQUEST_LABELS},
        LabeledMetricHistogram { BATCHER_LABELED_QUEUEING_TIMES_SECS, "batcher_labeled_queueing_times_secs", "Request queueing times of the batcher, per label (secs)", labels = BATCHER_REQUEST_LABELS},
        LabeledMetricHistogram { BATCHER_LABELED_LOCAL_RESPONSE_TIMES_SECS, "batcher_labeled_local_response_times_secs", "Request local response times of the batcher, per label (secs)", labels = BATCHER_REQUEST_LABELS},
        LabeledMetricHistogram { BATCHER_LABELED_REMOTE_RESPONSE_TIMES_SECS, "batcher_labeled_remote_response_times_secs", "Request remote response times of the batcher, per label (secs)", labels = BATCHER_REQUEST_LABELS},
        LabeledMetricHistogram { BATCHER_LABELED_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES_SECS, "batcher_labeled_remote_client_communication_failure_times_secs", "Request communication failure times of the batcher, per label (secs)", labels = BATCHER_REQUEST_LABELS},
    },
);

pub fn register_metrics(storage_height: BlockNumber) {
    STORAGE_HEIGHT.register();
    STORAGE_HEIGHT.set_lossy(storage_height.0);
    LAST_BATCHED_BLOCK.register();
    LAST_SYNCED_BLOCK.register();
    LAST_PROPOSED_BLOCK.register();
    REVERTED_BLOCKS.register();

    PROPOSAL_STARTED.register();
    PROPOSAL_SUCCEEDED.register();
    PROPOSAL_FAILED.register();
    PROPOSAL_ABORTED.register();

    BATCHED_TRANSACTIONS.register();
    REJECTED_TRANSACTIONS.register();
    REVERTED_TRANSACTIONS.register();
    SYNCED_TRANSACTIONS.register();

    FULL_BLOCKS.register();
    PRECONFIRMED_BLOCK_WRITTEN.register();

    // Blockifier's metrics
    CALLS_RUNNING_NATIVE.register();
    CLASS_CACHE_HITS.register();
    CLASS_CACHE_MISSES.register();
    NATIVE_CLASS_RETURNED.register();
    NATIVE_COMPILATION_ERROR.register();
    TOTAL_CALLS.register();
}

/// A handle to update the proposal metrics when the proposal is created and dropped.
#[derive(Debug)]
pub(crate) struct ProposalMetricsHandle {
    finish_status: ProposalFinishStatus,
}

impl ProposalMetricsHandle {
    pub fn new() -> Self {
        PROPOSAL_STARTED.increment(1);
        Self { finish_status: ProposalFinishStatus::Failed }
    }

    pub fn set_succeeded(&mut self) {
        self.finish_status = ProposalFinishStatus::Succeeded;
    }

    pub fn set_aborted(&mut self) {
        self.finish_status = ProposalFinishStatus::Aborted;
    }
}

#[derive(Debug)]
enum ProposalFinishStatus {
    Succeeded,
    Aborted,
    Failed,
}

impl Drop for ProposalMetricsHandle {
    fn drop(&mut self) {
        match self.finish_status {
            ProposalFinishStatus::Succeeded => PROPOSAL_SUCCEEDED.increment(1),
            ProposalFinishStatus::Aborted => PROPOSAL_ABORTED.increment(1),
            ProposalFinishStatus::Failed => PROPOSAL_FAILED.increment(1),
        }
    }
}

pub const _BATCHER_INFRA_METRICS: InfraMetrics = InfraMetrics {
    local_client_metrics: LocalClientMetrics::new(&BATCHER_LABELED_LOCAL_RESPONSE_TIMES_SECS),
    remote_client_metrics: RemoteClientMetrics::new(
        &BATCHER_REMOTE_CLIENT_SEND_ATTEMPTS,
        &BATCHER_LABELED_REMOTE_RESPONSE_TIMES_SECS,
        &BATCHER_LABELED_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES_SECS,
    ),
    local_server_metrics: LocalServerMetrics::new(
        &BATCHER_LOCAL_MSGS_RECEIVED,
        &BATCHER_LOCAL_MSGS_PROCESSED,
        &BATCHER_LOCAL_QUEUE_DEPTH,
        &BATCHER_LABELED_PROCESSING_TIMES_SECS,
        &BATCHER_LABELED_QUEUEING_TIMES_SECS,
    ),
    remote_server_metrics: RemoteServerMetrics::new(
        &BATCHER_REMOTE_MSGS_RECEIVED,
        &BATCHER_REMOTE_VALID_MSGS_RECEIVED,
        &BATCHER_REMOTE_MSGS_PROCESSED,
        &BATCHER_REMOTE_NUMBER_OF_CONNECTIONS,
    ),
};
