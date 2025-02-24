use starknet_api::block::BlockNumber;
use starknet_sequencer_metrics::metric_definitions::{
    BATCHED_TRANSACTIONS,
    PROPOSAL_ABORTED,
    PROPOSAL_FAILED,
    PROPOSAL_STARTED,
    PROPOSAL_SUCCEEDED,
    REJECTED_TRANSACTIONS,
    STORAGE_HEIGHT,
};

pub fn register_metrics(storage_height: BlockNumber) {
    STORAGE_HEIGHT.register();
    #[allow(clippy::as_conversions)]
    STORAGE_HEIGHT.set(storage_height.0 as f64);

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
