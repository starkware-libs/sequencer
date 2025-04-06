use apollo_metrics::define_metrics;
use apollo_metrics::metrics::MetricCounter;

define_metrics!(
    Consensus => {
        MetricCounter { L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY, "l1_gas_price_provider_insufficient_history", "Number of times the L1 gas price provider calculated an average with too few blocks", init=0},
    }
);

pub(crate) fn register_provider_metrics() {
    L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY.register();
}
