use apollo_metrics::define_metrics;

define_metrics!(
    Reverts => {
        MetricGauge { REVERTED_BATCHER_UP_TO_AND_INCLUDING, "reverted_batcher_up_to_and_including", "The block number up to which the batcher has reverted"},
        MetricGauge { REVERTED_STATE_SYNC_UP_TO_AND_INCLUDING, "reverted_state_sync_up_to_and_including", "The block number up to which the state sync has reverted" },
    },
);
