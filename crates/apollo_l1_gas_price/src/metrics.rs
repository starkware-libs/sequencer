use apollo_metrics::define_metrics;

define_metrics!(
    Consensus => {
        MetricCounter { L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY, "l1_gas_price_provider_insufficient_history", "Number of times the L1 gas price provider calculated an average with too few blocks", init=0 },
        MetricCounter { L1_GAS_PRICE_SCRAPER_BASELAYER_ERROR_COUNT, "l1_gas_price_scraper_baselayer_error_count", "Number of times the L1 gas price scraper encountered an error while scraping the base layer", init=0 },
        MetricCounter { L1_GAS_PRICE_SCRAPER_REORG_DETECTED, "l1_gas_price_scraper_reorg_detected", "Number of times the L1 gas price scraper detected a reorganization in the base layer", init=0 },
        MetricCounter { ETH_TO_STRK_ERROR_COUNT, "eth_to_strk_error_count", "Number of times the query to the Eth to Strk oracle failed due to an error or timeout", init=0 },
    }
);

pub(crate) fn register_provider_metrics() {
    L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY.register();
}

pub(crate) fn register_scraper_metrics() {
    L1_GAS_PRICE_SCRAPER_BASELAYER_ERROR_COUNT.register();
    L1_GAS_PRICE_SCRAPER_REORG_DETECTED.register();
}

pub(crate) fn register_eth_to_strk_metrics() {
    ETH_TO_STRK_ERROR_COUNT.register();
}
