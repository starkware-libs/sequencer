use apollo_batcher_types::communication::BATCHER_REQUEST_LABELS;
use apollo_infra::metrics::{
    InfraMetrics,
    LocalClientMetrics,
    LocalServerMetrics,
    RemoteClientMetrics,
    RemoteServerMetrics,
};
use apollo_metrics::{define_infra_metrics, define_metrics, generate_permutation_labels};
use blockifier::metrics::{
    CacheMetrics,
    CALLS_RUNNING_NATIVE,
    NATIVE_CLASS_RETURNED,
    NATIVE_COMPILATION_ERROR,
    TOTAL_CALLS,
};
use starknet_api::block::BlockNumber;
use strum::{EnumVariantNames, VariantNames};
use strum_macros::IntoStaticStr;

define_infra_metrics!(batcher);

define_metrics!(
    Batcher => {
        // Global class cache
        MetricCounter { CLASS_CACHE_MISSES, "batcher_class_cache_misses", "Counter of the batcher's global class cache misses", init=0 },
        MetricCounter { CLASS_CACHE_HITS, "batcher_class_cache_hits", "Counter of the batcher's global class cache hits", init=0 },
        // Heights
        MetricGauge { BUILDING_HEIGHT, "batcher_building_height", "The height of the block that should be built next. The height of the state diff marker as stored in the batcher's storage." },
        MetricGauge { GLOBAL_ROOT_HEIGHT, "batcher_global_root_height", "The height of the first block without global root stored." },
        MetricGauge { LAST_BATCHED_BLOCK_HEIGHT, "batcher_last_batched_block_height", "The height of the last block received by batching" },
        MetricGauge { LAST_SYNCED_BLOCK_HEIGHT, "batcher_last_synced_block_height", "The height of the last block received by syncing" },
        MetricGauge { LAST_PROPOSED_BLOCK_HEIGHT, "batcher_last_proposed_block_height", "The height of the last block proposed by this sequencer" },
        MetricCounter { REVERTED_BLOCKS, "batcher_reverted_blocks", "Counter of reverted blocks", init = 0 },
        // Proposals
        MetricCounter { PROPOSAL_STARTED, "batcher_proposal_started", "Counter of proposals started", init = 0 },
        MetricCounter { PROPOSAL_SUCCEEDED, "batcher_proposal_succeeded", "Counter of successful proposals", init = 0 },
        MetricCounter { PROPOSAL_FAILED, "batcher_proposal_failed", "Counter of failed proposals", init = 0 },
        MetricCounter { PROPOSAL_ABORTED, "batcher_proposal_aborted", "Counter of aborted proposals", init = 0 },
        // Per-proposal txs that did not end up in block or were deferred
        MetricGauge { VALIDATOR_WASTED_TXS, "batcher_validator_wasted_txs", "Number of txs executed by validator but not included in the block"},
        MetricGauge { PROPOSER_DEFERRED_TXS, "batcher_proposer_deferred_txs", "Number of txs started execution but not finished by end of proposal by proposer"},
        // Transactions
        MetricCounter { BATCHED_TRANSACTIONS, "batcher_batched_transactions", "Counter of batched transactions across all forks", init = 0 },
        MetricCounter { REJECTED_TRANSACTIONS, "batcher_rejected_transactions", "Counter of rejected transactions", init = 0 },
        MetricCounter { REVERTED_TRANSACTIONS, "batcher_reverted_transactions", "Counter of reverted transactions across all forks", init = 0 },
        MetricCounter { SYNCED_TRANSACTIONS, "batcher_synced_transactions", "Counter of synced transactions", init = 0 },
        MetricHistogram { NUM_TRANSACTION_IN_BLOCK, "batcher_num_transaction_in_block", "Number of transactions in a block"},

        MetricCounter { BATCHER_L1_PROVIDER_ERRORS, "batcher_l1_provider_errors", "Counter of L1 provider errors", init = 0 },
        MetricCounter { PRECONFIRMED_BLOCK_WRITTEN, "batcher_preconfirmed_block_written", "Counter of preconfirmed blocks written to storage", init = 0 },
        // Block close reason
        LabeledMetricCounter { BLOCK_CLOSE_REASON, "batcher_block_close_reason", "Number of blocks closed by reason", init = 0 , labels = BLOCK_CLOSE_REASON_LABELS},
        // Block weights
        // TODO(MatanL): Consider using histogram for these metrics.
        MetricGauge { SIERRA_GAS_IN_LAST_BLOCK, "batcher_sierra_gas_in_last_block", "The sierra gas in the last block"},
        MetricGauge { PROVING_GAS_IN_LAST_BLOCK, "batcher_proving_gas_in_last_block", "The proving gas in the last block"},
        MetricGauge { L2_GAS_IN_LAST_BLOCK, "batcher_l2_gas_in_last_block", "The L2 gas used in the last block"},
        // Commitment manager
        MetricHistogram { COMMITMENT_MANAGER_COMMIT_BLOCK_LATENCY, "batcher_commitment_manager_commit_block_latency", "The latency of commit tasks in the commitment manager in seconds" },
        MetricHistogram { COMMITMENT_MANAGER_REVERT_BLOCK_LATENCY, "batcher_commitment_manager_revert_block_latency", "The latency of revert tasks in the commitment manager in seconds" },
        MetricHistogram { COMMITMENT_MANAGER_NUM_COMMIT_RESULTS, "batcher_commitment_manager_num_commit_results", "The number of commit results received from the commitment manager" },
        // Block commitment components computation timings
        MetricHistogram { TX_COMMITMENT_LATENCY, "batcher_tx_commitment_latency", "Duration of transaction commitment computation in seconds" },
        MetricHistogram { TX_COMMITMENT_PER_TX_LATENCY, "batcher_tx_commitment_per_tx_latency", "Duration of transactions commitment computation per transaction in seconds" },
        MetricHistogram { EVENT_COMMITMENT_LATENCY, "batcher_event_commitment_latency", "Duration of event commitment computation in seconds" },
        MetricHistogram { EVENT_COMMITMENT_PER_EVENT_LATENCY, "batcher_event_commitment_per_event_latency", "Duration of event commitment computation per event in seconds" },
        MetricHistogram { RECEIPT_COMMITMENT_LATENCY, "batcher_receipt_commitment_latency", "Duration of receipt commitment computation in seconds" },
        MetricHistogram { STATE_DIFF_COMMITMENT_LATENCY, "batcher_state_diff_commitment_latency", "Duration of state diff commitment computation in seconds" },
        MetricHistogram { STATE_DIFF_COMMITMENT_PER_STATE_DIFF_LENGTH_LATENCY, "batcher_state_diff_commitment_per_state_diff_length", "Duration of state diff commitment computation per state diff length in seconds" },
    },
);

pub const LABEL_NAME_BLOCK_CLOSE_REASON: &str = "block_close_reason";

#[derive(Clone, Copy, Debug, IntoStaticStr, EnumVariantNames)]
#[strum(serialize_all = "snake_case")]
pub enum BlockCloseReason {
    FullBlock,
    Deadline,
    /// Block building finished because no new transactions are being executed and the configured
    /// timeout passed.
    IdleExecutionTimeout,
}

generate_permutation_labels! {
    BLOCK_CLOSE_REASON_LABELS,
    (LABEL_NAME_BLOCK_CLOSE_REASON, BlockCloseReason),
}

pub(crate) fn record_block_close_reason(reason: BlockCloseReason) {
    BLOCK_CLOSE_REASON.increment(1, &[(LABEL_NAME_BLOCK_CLOSE_REASON, reason.into())]);
}

pub const BATCHER_CLASS_CACHE_METRICS: CacheMetrics =
    CacheMetrics::new(CLASS_CACHE_MISSES, CLASS_CACHE_HITS);

pub fn register_metrics(storage_height: BlockNumber, global_root_height: BlockNumber) {
    BUILDING_HEIGHT.register();
    BUILDING_HEIGHT.set_lossy(storage_height.0);
    GLOBAL_ROOT_HEIGHT.register();
    GLOBAL_ROOT_HEIGHT.set_lossy(global_root_height.0);
    LAST_BATCHED_BLOCK_HEIGHT.register();
    LAST_SYNCED_BLOCK_HEIGHT.register();
    LAST_PROPOSED_BLOCK_HEIGHT.register();
    REVERTED_BLOCKS.register();

    PROPOSAL_STARTED.register();
    PROPOSAL_SUCCEEDED.register();
    PROPOSAL_FAILED.register();
    PROPOSAL_ABORTED.register();
    VALIDATOR_WASTED_TXS.register();
    PROPOSER_DEFERRED_TXS.register();

    BATCHED_TRANSACTIONS.register();
    REJECTED_TRANSACTIONS.register();
    REVERTED_TRANSACTIONS.register();
    SYNCED_TRANSACTIONS.register();

    BATCHER_L1_PROVIDER_ERRORS.register();
    PRECONFIRMED_BLOCK_WRITTEN.register();
    BLOCK_CLOSE_REASON.register();
    NUM_TRANSACTION_IN_BLOCK.register();

    SIERRA_GAS_IN_LAST_BLOCK.register();
    PROVING_GAS_IN_LAST_BLOCK.register();
    L2_GAS_IN_LAST_BLOCK.register();

    COMMITMENT_MANAGER_COMMIT_BLOCK_LATENCY.register();
    COMMITMENT_MANAGER_REVERT_BLOCK_LATENCY.register();
    COMMITMENT_MANAGER_NUM_COMMIT_RESULTS.register();

    TX_COMMITMENT_LATENCY.register();
    TX_COMMITMENT_PER_TX_LATENCY.register();
    EVENT_COMMITMENT_LATENCY.register();
    EVENT_COMMITMENT_PER_EVENT_LATENCY.register();
    RECEIPT_COMMITMENT_LATENCY.register();
    STATE_DIFF_COMMITMENT_LATENCY.register();
    STATE_DIFF_COMMITMENT_PER_STATE_DIFF_LENGTH_LATENCY.register();

    // Blockifier's metrics
    BATCHER_CLASS_CACHE_METRICS.register();
    CALLS_RUNNING_NATIVE.register();
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
