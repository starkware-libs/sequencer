use apollo_metrics::define_metrics;

define_metrics!(
    L1ToL2Message => {
        MetricCounter { L1_MESSAGE_SCRAPER_BASELAYER_ERROR_COUNT, "l1_message_scraper_baselayer_error_count", "Number of times the L1 message scraper encountered an error while scraping the base layer", init=0},
    }
);

pub(crate) fn register_scraper_metrics() {
    L1_MESSAGE_SCRAPER_BASELAYER_ERROR_COUNT.register();
}
