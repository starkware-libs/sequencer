use starknet_api::block::BlockNumber;
use apollo_sequencer_metrics::define_metrics;
use apollo_sequencer_metrics::metrics::{MetricCounter, MetricGauge};

define_metrics!(
    Batcher => {
        // Gauges
        MetricGauge { STORAGE_HEIGHT, "batcher_storage_height", "The height of the batcher's storage" },
        // Counters
        MetricCounter { PROPOSAL_STARTED, "batcher_proposal_started", "Counter of proposals started", init = 0 },
        MetricCounter { CLASS_CACHE_MISSES, "class_cache_misses", "Counter of global class cache misses", init=0 },
        MetricCounter { CLASS_CACHE_HITS, "class_cache_hits", "Counter of global class cache hits", init=0 },
        MetricCounter { PROPOSAL_SUCCEEDED, "batcher_proposal_succeeded", "Counter of successful proposals", init = 0 },
        MetricCounter { PROPOSAL_FAILED, "batcher_proposal_failed", "Counter of failed proposals", init = 0 },
        MetricCounter { PROPOSAL_ABORTED, "batcher_proposal_aborted", "Counter of aborted proposals", init = 0 },
        MetricCounter { BATCHED_TRANSACTIONS, "batcher_batched_transactions", "Counter of batched transactions across all forks", init = 0 },
        MetricCounter { REJECTED_TRANSACTIONS, "batcher_rejected_transactions", "Counter of rejected transactions", init = 0 },
        MetricCounter { SYNCED_BLOCKS, "batcher_synced_blocks", "Counter of synced blocks", init = 0 },
        MetricCounter { SYNCED_TRANSACTIONS, "batcher_synced_transactions", "Counter of synced transactions", init = 0 },
        MetricCounter { REVERTED_BLOCKS, "batcher_reverted_blocks", "Counter of reverted blocks", init = 0 }
    },
);

pub fn register_metrics(storage_height: BlockNumber) {
    STORAGE_HEIGHT.register();
    STORAGE_HEIGHT.set_lossy(storage_height.0);
    CLASS_CACHE_MISSES.register();
    CLASS_CACHE_HITS.register();

    PROPOSAL_STARTED.register();
    PROPOSAL_SUCCEEDED.register();
    PROPOSAL_FAILED.register();
    PROPOSAL_ABORTED.register();

    // In case of revert, consider calling `absolute`.
    BATCHED_TRANSACTIONS.register();
    REJECTED_TRANSACTIONS.register();
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
