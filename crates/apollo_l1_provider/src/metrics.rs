use apollo_infra::metrics::{
    InfraMetrics,
    LocalClientMetrics,
    LocalServerMetrics,
    RemoteClientMetrics,
    RemoteServerMetrics,
};
use apollo_l1_provider_types::L1_PROVIDER_REQUEST_LABELS;
use apollo_metrics::{define_infra_metrics, define_metrics};

define_infra_metrics!(l1_provider);

define_metrics!(
    L1Provider => {
        MetricGauge { L1_MESSAGE_SCRAPER_LATEST_SCRAPED_BLOCK, "l1_message_scraper_latest_scraped_block", "The latest block number that the L1 message scraper has scraped" },
        MetricCounter { L1_MESSAGE_SCRAPER_SUCCESS_COUNT, "l1_message_scraper_success_count", "Number of times the L1 message scraper successfully scraped messages and updated the provider", init=0 },
        MetricCounter { L1_MESSAGE_SCRAPER_BASELAYER_ERROR_COUNT, "l1_message_scraper_baselayer_error_count", "Number of times the L1 message scraper encountered an error while scraping the base layer", init=0},
        MetricCounter { L1_MESSAGE_SCRAPER_REORG_DETECTED, "l1_message_scraper_reorg_detected", "Number of times the L1 message scraper detected a reorganization in the base layer", init=0},
        MetricGauge { L1_MESSAGE_SCRAPER_LAST_SUCCESS_TIMESTAMP_SECONDS, "l1_message_scraper_last_success_timestamp_seconds", "Unix timestamp (seconds) of the last successful L1 message scrape" },
        MetricGauge { L1_PROVIDER_NUM_PENDING_TXS, "l1_provider_num_pending_txs", "The number of pending L1 handler transactions in the transaction manager" },
    },
);

pub(crate) fn register_scraper_metrics() {
    L1_MESSAGE_SCRAPER_LATEST_SCRAPED_BLOCK.register();
    L1_MESSAGE_SCRAPER_SUCCESS_COUNT.register();
    L1_MESSAGE_SCRAPER_BASELAYER_ERROR_COUNT.register();
    L1_MESSAGE_SCRAPER_REORG_DETECTED.register();
    L1_MESSAGE_SCRAPER_LAST_SUCCESS_TIMESTAMP_SECONDS.register();
}

pub(crate) fn register_provider_metrics() {
    L1_PROVIDER_NUM_PENDING_TXS.register();
}
