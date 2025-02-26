use starknet_sequencer_metrics::define_metrics;
use starknet_sequencer_metrics::metrics::MetricGauge;

define_metrics!(
    Consensus => {
        MetricGauge { CONSENSUS_HEIGHT, "consensus_height", "The block number consensus is working to decide" },
    },
);

pub(crate) fn register_metrics() {
    CONSENSUS_HEIGHT.register();
}
