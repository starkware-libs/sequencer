use starknet_api::block::BlockNumber;
use starknet_sequencer_metrics::metrics::{MetricCounter, MetricGauge};

// Height metrics.
pub const STORAGE_HEIGHT: MetricGauge =
    MetricGauge::new("batcher_storage_height", "The height of the batcher's storage");

// Proposal metrics.
pub const PROPOSAL_STARTED: MetricCounter =
    MetricCounter::new("batcher_proposal_started", "Counter of proposals started", 0);
pub const PROPOSAL_SUCCEEDED: MetricCounter =
    MetricCounter::new("batcher_proposal_succeeded", "Counter of successful proposals", 0);
pub const PROPOSAL_FAILED: MetricCounter =
    MetricCounter::new("batcher_proposal_failed", "Counter of failed proposals", 0);
pub const PROPOSAL_ABORTED: MetricCounter =
    MetricCounter::new("batcher_proposal_aborted", "Counter of aborted proposals", 0);

// Transaction metrics.
pub const BATCHED_TRANSACTIONS: MetricCounter =
    MetricCounter::new("batcher_batched_transactions", "Counter of batched transactions", 0);
pub const REJECTED_TRANSACTIONS: MetricCounter =
    MetricCounter::new("batcher_rejected_transactions", "Counter of rejected transactions", 0);

pub fn register_metrics(storage_height: BlockNumber) {
    let storage_height_metric = STORAGE_HEIGHT.register();
    #[allow(clippy::as_conversions)]
    storage_height_metric.set(storage_height.0 as f64);

    PROPOSAL_STARTED.register();
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
