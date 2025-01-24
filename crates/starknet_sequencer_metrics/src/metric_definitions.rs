use crate::metrics::{MetricCounter, MetricGauge};

// ~~~ HTTP SERVER METRICS START ~~~ //

pub const ADDED_TRANSACTIONS_TOTAL: MetricCounter =
    MetricCounter::new("ADDED_TRANSACTIONS_TOTAL", "Total number of transactions added", 0);
pub const ADDED_TRANSACTIONS_SUCCESS: MetricCounter = MetricCounter::new(
    "ADDED_TRANSACTIONS_SUCCESS",
    "Number of successfully added transactions",
    0,
);
pub const ADDED_TRANSACTIONS_FAILURE: MetricCounter =
    MetricCounter::new("ADDED_TRANSACTIONS_FAILURE", "Number of faulty added transactions", 0);

// ~~~ HTTP SERVER METRICS END ~~~ //

// ~~~ BATCHER METRICS START ~~~ //

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

// ~~~ BATCHER METRICS END ~~~ //
