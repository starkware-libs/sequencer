use starknet_sequencer_metrics::define_metrics;
use starknet_sequencer_metrics::metrics::{MetricGauge, MetricScope};

define_metrics!(
    Consensus => {
        MetricGauge { CONSENSUS_HEIGHT, "consensus_height", "The block number consensus is working to decide" },
        MetricGauge { CONSENSUS_ROUND, "consensus_round", "The round of the state machine"}
    },
);

pub(crate) fn register_metrics() {
    CONSENSUS_HEIGHT.register();
    CONSENSUS_ROUND.register();
}
