use apollo_metrics::define_metrics;
use apollo_metrics::metrics::MetricCounter;

define_metrics!(
    Consensus => {
        MetricCounter { L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY, "l1_gas_price_provider_insufficient_history", "Number of times the L1 gas price provider calculated an average with too short block history", init=0},
        MetricCounter { L1_GAS_PRICE_SCRAPER_STARTUP_NO_HISTORY, "l1_gas_price_scraper_startup_no_history", "Number of times the L1 gas price scraper started running when there was no block history", init=0},
    }
);

pub(crate) fn register_provider_metrics() {
    L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY.register();
}

pub(crate) fn register_scraper_metrics() {
    L1_GAS_PRICE_SCRAPER_STARTUP_NO_HISTORY.register();
}
