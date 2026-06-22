use apollo_infra::metrics::{
    InfraMetrics,
    LocalClientMetrics,
    LocalServerMetrics,
    RemoteClientMetrics,
    RemoteServerMetrics,
};
use apollo_l1_events_types::L1_EVENTS_REQUEST_LABELS;
use apollo_metrics::{define_infra_metrics, define_metrics};

define_infra_metrics!(l1_events);

define_metrics!(
    L1EventsProvider => {
        MetricGauge { L1_MESSAGE_SCRAPER_LATEST_SCRAPED_BLOCK, "l1_message_scraper_latest_scraped_block", "The latest block number that the L1 message scraper has scraped" },
        MetricCounter { L1_MESSAGE_SCRAPER_SUCCESS_COUNT, "l1_message_scraper_success_count", "Number of times the L1 message scraper successfully scraped messages and updated the provider", init=0 },
        MetricCounter { L1_MESSAGE_SCRAPER_L1_HANDLER_TX_COUNT, "l1_message_scraper_l1_handler_tx_count", "Number of unique L1 handler transactions scraped from L1 (Full payload stored)", init=0 },
        MetricCounter { L1_MESSAGE_SCRAPER_BASELAYER_ERROR_COUNT, "l1_message_scraper_baselayer_error_count", "Number of times the L1 message scraper encountered an error while scraping the base layer", init=0},
        MetricCounter { L1_MESSAGE_SCRAPER_REORG_DETECTED, "l1_message_scraper_reorg_detected", "Number of times the L1 message scraper detected a reorganization in the base layer", init=0},
        MetricGauge { L1_MESSAGE_SCRAPER_LAST_SUCCESS_TIMESTAMP_SECONDS, "l1_message_scraper_last_success_timestamp_seconds", "Unix timestamp (seconds) of the last successful L1 message scrape" },
        MetricGauge { L1_MESSAGE_PROVIDER_NUM_PENDING_TXS, "l1_message_provider_num_pending_txs", "The number of pending L1 handler transactions in the transaction manager" },
        MetricGauge { L1_MESSAGE_PROVIDER_OLDEST_PENDING_TX_L1_TIMESTAMP_SECONDS, "l1_message_provider_oldest_pending_tx_l1_timestamp_seconds", "The L1 block timestamp (unix seconds) of the oldest pending (uncommitted) L1 handler transaction; 0 when none are pending" },
        MetricGauge { L1_MESSAGE_PROVIDER_COMMIT_BLOCK_BACKLOG_LEN, "l1_message_provider_commit_block_backlog_len", "The number of commit-blocks buffered in the catch-up backlog while the provider syncs to the target height; abnormal sustained growth indicates a stalled or lagging L2 sync" },
    },
);

pub(crate) fn register_scraper_metrics() {
    L1_MESSAGE_SCRAPER_LATEST_SCRAPED_BLOCK.register();
    L1_MESSAGE_SCRAPER_SUCCESS_COUNT.register();
    L1_MESSAGE_SCRAPER_L1_HANDLER_TX_COUNT.register();
    L1_MESSAGE_SCRAPER_BASELAYER_ERROR_COUNT.register();
    L1_MESSAGE_SCRAPER_REORG_DETECTED.register();
    L1_MESSAGE_SCRAPER_LAST_SUCCESS_TIMESTAMP_SECONDS.register();
}

pub(crate) fn register_provider_metrics() {
    L1_MESSAGE_PROVIDER_NUM_PENDING_TXS.register();
    L1_MESSAGE_PROVIDER_OLDEST_PENDING_TX_L1_TIMESTAMP_SECONDS.register();
    L1_MESSAGE_PROVIDER_COMMIT_BLOCK_BACKLOG_LEN.register();
}
