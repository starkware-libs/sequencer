use apollo_metrics::define_metrics;

define_metrics!(
    L1Provider => {
        MetricCounter { L1_MESSAGE_SCRAPER_SUCCESS_COUNT, "l1_message_scraper_success_count", "Number of times the L1 message scraper successfully scraped messages and updated the provider", init=0 },
        MetricCounter { L1_MESSAGE_SCRAPER_BASELAYER_ERROR_COUNT, "l1_message_scraper_baselayer_error_count", "Number of times the L1 message scraper encountered an error while scraping the base layer", init=0},
        MetricCounter { L1_MESSAGE_SCRAPER_REORG_DETECTED, "l1_message_scraper_reorg_detected", "Number of times the L1 message scraper detected a reorganization in the base layer", init=0},
    }
);

pub(crate) fn register_scraper_metrics() {
    L1_MESSAGE_SCRAPER_SUCCESS_COUNT.register();
    L1_MESSAGE_SCRAPER_BASELAYER_ERROR_COUNT.register();
    L1_MESSAGE_SCRAPER_REORG_DETECTED.register();
}
