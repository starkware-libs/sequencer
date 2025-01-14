use metrics::{counter, describe_counter};
use starknet_api::block::BlockNumber;

pub const STORAGE_HEIGHT: Metric =
    Metric { name: "batcher_storage_height", description: "The height of the batcher storage" };
pub const PROPOSAL_STARTED: Metric =
    Metric { name: "batcher_proposal_started", description: "Counter of proposals started" };
pub const PROPOSAL_SUCCEEDED: Metric =
    Metric { name: "batcher_proposal_succeeded", description: "Counter of successful proposals" };
pub const PROPOSAL_FAILED: Metric =
    Metric { name: "batcher_proposal_failed", description: "Counter of failed proposals" };
pub const PROPOSAL_ABORTED: Metric =
    Metric { name: "batcher_proposal_aborted", description: "Counter of aborted proposals" };

pub struct Metric {
    pub name: &'static str,
    pub description: &'static str,
}

pub fn register_metrics(storage_height: BlockNumber) {
    // Ideally, we would have a `Gauge` here because of reverts, but we can't because
    // the value will need to implement `Into<f64>` and `BlockNumber` doesn't (it is u64).
    // In case of reverts, consider calling `absolute`.
    counter!(STORAGE_HEIGHT.name).absolute(storage_height.0);
    describe_counter!(STORAGE_HEIGHT.name, STORAGE_HEIGHT.description);

    counter!(PROPOSAL_STARTED.name).absolute(0);
    describe_counter!(PROPOSAL_STARTED.name, PROPOSAL_STARTED.description);
    counter!(PROPOSAL_SUCCEEDED.name).absolute(0);
    describe_counter!(PROPOSAL_SUCCEEDED.name, PROPOSAL_SUCCEEDED.description);
    counter!(PROPOSAL_FAILED.name).absolute(0);
    describe_counter!(PROPOSAL_FAILED.name, PROPOSAL_FAILED.description);
    counter!(PROPOSAL_ABORTED.name).absolute(0);
    describe_counter!(PROPOSAL_ABORTED.name, PROPOSAL_ABORTED.description);
}
