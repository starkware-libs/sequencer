use metrics::{counter, describe_counter, describe_gauge, gauge};
use starknet_api::block::BlockNumber;

// Height metrics.
pub const STORAGE_HEIGHT: Metric =
    Metric { name: "batcher_storage_height", description: "The height of the batcher's storage" };

// Proposal metrics.
pub const PROPOSAL_STARTED: Metric =
    Metric { name: "batcher_proposal_started", description: "Counter of proposals started" };
pub const PROPOSAL_SUCCEEDED: Metric =
    Metric { name: "batcher_proposal_succeeded", description: "Counter of successful proposals" };
pub const PROPOSAL_FAILED: Metric =
    Metric { name: "batcher_proposal_failed", description: "Counter of failed proposals" };
pub const PROPOSAL_ABORTED: Metric =
    Metric { name: "batcher_proposal_aborted", description: "Counter of aborted proposals" };

// Transaction metrics.
pub const BATCHED_TRANSACTIONS: Metric =
    Metric { name: "batcher_batched_transactions", description: "Counter of batched transactions" };
pub const REJECTED_TRANSACTIONS: Metric = Metric {
    name: "batcher_rejected_transactions",
    description: "Counter of rejected transactions",
};

pub struct Metric {
    pub name: &'static str,
    pub description: &'static str,
}

pub fn register_metrics(storage_height: BlockNumber) {
    let storage_height_metric = gauge!(STORAGE_HEIGHT.name);
    describe_gauge!(STORAGE_HEIGHT.name, STORAGE_HEIGHT.description);
    #[allow(clippy::as_conversions)]
    storage_height_metric.set(storage_height.0 as f64);

    counter!(PROPOSAL_STARTED.name).absolute(0);
    describe_counter!(PROPOSAL_STARTED.name, PROPOSAL_STARTED.description);
    counter!(PROPOSAL_SUCCEEDED.name).absolute(0);
    describe_counter!(PROPOSAL_SUCCEEDED.name, PROPOSAL_SUCCEEDED.description);
    counter!(PROPOSAL_FAILED.name).absolute(0);
    describe_counter!(PROPOSAL_FAILED.name, PROPOSAL_FAILED.description);
    counter!(PROPOSAL_ABORTED.name).absolute(0);
    describe_counter!(PROPOSAL_ABORTED.name, PROPOSAL_ABORTED.description);

    // In case of revert, consider calling `absolute`.
    counter!(BATCHED_TRANSACTIONS.name).absolute(0);
    describe_counter!(BATCHED_TRANSACTIONS.name, BATCHED_TRANSACTIONS.description);
    counter!(REJECTED_TRANSACTIONS.name).absolute(0);
    describe_counter!(REJECTED_TRANSACTIONS.name, REJECTED_TRANSACTIONS.description);
}

/// A handle to update the proposal metrics when the proposal is created and dropped.
#[derive(Debug)]
pub(crate) struct ProposalMetricsHandle {
    finish_status: ProposalFinishStatus,
}

impl ProposalMetricsHandle {
    pub fn new() -> Self {
        counter!(crate::metrics::PROPOSAL_STARTED.name).increment(1);
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
            ProposalFinishStatus::Succeeded => {
                counter!(crate::metrics::PROPOSAL_SUCCEEDED.name).increment(1)
            }
            ProposalFinishStatus::Aborted => {
                counter!(crate::metrics::PROPOSAL_ABORTED.name).increment(1)
            }
            ProposalFinishStatus::Failed => {
                counter!(crate::metrics::PROPOSAL_FAILED.name).increment(1)
            }
        }
    }
}
