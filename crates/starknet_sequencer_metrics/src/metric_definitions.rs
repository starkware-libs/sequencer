use crate::metrics::{MetricCounter, MetricGauge, MetricScope};

#[cfg(test)]
#[path = "metric_definitions_test.rs"]
pub mod metric_definitions_test;

/// Macro to define `MetricCounter` constants for specified scopes and store them in a collection.
/// This generates:
/// - Individual `MetricCounter` constants (e.g., `PROPOSAL_STARTED`).
/// - A const array `ALL_METRIC_COUNTERS` containing all defined `MetricCounter` constants.
macro_rules! define_counter_metrics {
    (
        $(
            $scope:expr => {
                $(
                    { $name:ident, $key:expr, $desc:expr, $init:expr }
                ),*
                $(,)?
            }
        ),*
        $(,)?
    ) => {
        $(
            $(
                pub const $name: MetricCounter = MetricCounter::new(
                    $scope,
                    $key,
                    $desc,
                    $init
                );
            )*
        )*

        pub const ALL_METRIC_COUNTERS: &[MetricCounter] = &[
            $(
                $($name),*
            ),*
        ];
    };
}

/// Macro to define `MetricGauge` constants for specified scopes and store them in a collection.
/// This generates:
/// - Individual `MetricGauge` constants (e.g., `STORAGE_HEIGHT`).
/// - A `const` array `ALL_METRIC_GAUGES` containing all defined `MetricGauge` constants.
macro_rules! define_gauge_metrics {
    (
        $(
            $scope:expr => {
                $(
                    { $name:ident, $key:expr, $desc:expr }
                ),*
                $(,)?
            }
        ),*
        $(,)?
    ) => {
        $(
            $(
                pub const $name: MetricGauge = MetricGauge::new(
                    $scope,
                    $key,
                    $desc
                );
            )*
        )*

        pub const ALL_METRIC_GAUGES: &[MetricGauge] = &[
            $(
                $($name),*
            ),*
        ];
    };
}

define_gauge_metrics!(
    MetricScope::Batcher => {
        { STORAGE_HEIGHT, "batcher_storage_height", "The height of the batcher's storage" }
    },
);

define_counter_metrics!(
    MetricScope::Batcher => {
        { PROPOSAL_STARTED, "batcher_proposal_started", "Counter of proposals started", 0 },
        { PROPOSAL_SUCCEEDED, "batcher_proposal_succeeded", "Counter of successful proposals", 0 },
        { PROPOSAL_FAILED, "batcher_proposal_failed", "Counter of failed proposals", 0 },
        { PROPOSAL_ABORTED, "batcher_proposal_aborted", "Counter of aborted proposals", 0 },
        { BATCHED_TRANSACTIONS, "batcher_batched_transactions", "Counter of batched transactions", 0 },
        { REJECTED_TRANSACTIONS, "batcher_rejected_transactions", "Counter of rejected transactions", 0 }
    },
    MetricScope::HttpServer => {
        { ADDED_TRANSACTIONS_TOTAL, "ADDED_TRANSACTIONS_TOTAL", "Total number of transactions added", 0 },
        { ADDED_TRANSACTIONS_SUCCESS, "ADDED_TRANSACTIONS_SUCCESS", "Number of successfully added transactions", 0 },
        { ADDED_TRANSACTIONS_FAILURE, "ADDED_TRANSACTIONS_FAILURE", "Number of faulty added transactions", 0 }
    },
);
