use apollo_metrics::define_metrics;
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
