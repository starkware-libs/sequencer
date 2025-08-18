use apollo_l1_gas_price_types::L1_GAS_PRICE_REQUEST_LABELS;
use apollo_metrics::define_metrics;

define_metrics!(
    L1GasPrice => {
        MetricCounter { L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY, "l1_gas_price_provider_insufficient_history", "Number of times the L1 gas price provider calculated an average with too few blocks", init=0 },
        MetricCounter { L1_GAS_PRICE_SCRAPER_SUCCESS_COUNT, "l1_gas_price_scraper_success_count", "Number of times the L1 gas price scraper successfully scraped and updated gas prices", init=0 },
        MetricCounter { L1_GAS_PRICE_SCRAPER_BASELAYER_ERROR_COUNT, "l1_gas_price_scraper_baselayer_error_count", "Number of times the L1 gas price scraper encountered an error while scraping the base layer", init=0 },
        MetricCounter { L1_GAS_PRICE_SCRAPER_REORG_DETECTED, "l1_gas_price_scraper_reorg_detected", "Number of times the L1 gas price scraper detected a reorganization in the base layer", init=0 },
        MetricCounter { ETH_TO_STRK_ERROR_COUNT, "eth_to_strk_error_count", "Number of times the query to the Eth to Strk oracle failed due to an error or timeout", init=0 },
        MetricCounter { ETH_TO_STRK_SUCCESS_COUNT, "eth_to_strk_success_count", "Number of times the query to the Eth to Strk oracle succeeded", init=0 },
        MetricGauge { L1_GAS_PRICE_SCRAPER_LATEST_SCRAPED_BLOCK, "l1_gas_price_scraper_latest_scraped_block", "The latest block number that the L1 gas price scraper has scraped" },
        MetricGauge { ETH_TO_STRK_RATE, "eth_to_strk_rate", "The current rate of ETH to STRK conversion" },
        MetricGauge { L1_GAS_PRICE_LATEST_MEAN_VALUE, "l1_gas_price_latest_mean_value", "The latest L1 gas price, calculated as an average by the provider client" },
        MetricGauge { L1_DATA_GAS_PRICE_LATEST_MEAN_VALUE, "l1_data_gas_price_latest_mean_value", "The latest L1 data gas price, calculated as an average by the provider client" }
    },
    Infra => {
        LabeledMetricHistogram {
            L1_GAS_PRICE_PROVIDER_LABELED_PROCESSING_TIMES_SECS,
            "l1_gas_price_labeled_processing_times_secs",
            "Request processing times of the L1 gas price, per label (secs)",
            labels = L1_GAS_PRICE_REQUEST_LABELS
        },
        LabeledMetricHistogram {
            L1_GAS_PRICE_PROVIDER_LABELED_QUEUEING_TIMES_SECS,
            "l1_gas_price_labeled_queueing_times_secs",
            "Request queueing times of the L1 gas price, per label (secs)",
            labels = L1_GAS_PRICE_REQUEST_LABELS
        },
        LabeledMetricHistogram {
            L1_GAS_PRICE_LABELED_LOCAL_RESPONSE_TIMES_SECS,
            "l1_gas_price_labeled_local_response_times_secs",
            "Request local response times of the L1 gas price, per label (secs)",
            labels = L1_GAS_PRICE_REQUEST_LABELS
        },
        LabeledMetricHistogram {
            L1_GAS_PRICE_LABELED_REMOTE_RESPONSE_TIMES_SECS,
            "l1_gas_price_labeled_remote_response_times_secs",
            "Request remote response times of the L1 gas price, per label (secs)",
            labels = L1_GAS_PRICE_REQUEST_LABELS
        },
    },
);

pub(crate) fn register_provider_metrics() {
    L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY.register();
    L1_GAS_PRICE_LATEST_MEAN_VALUE.register();
    L1_DATA_GAS_PRICE_LATEST_MEAN_VALUE.register();
}

pub(crate) fn register_scraper_metrics() {
    L1_GAS_PRICE_SCRAPER_SUCCESS_COUNT.register();
    L1_GAS_PRICE_SCRAPER_BASELAYER_ERROR_COUNT.register();
    L1_GAS_PRICE_SCRAPER_REORG_DETECTED.register();
    L1_GAS_PRICE_SCRAPER_LATEST_SCRAPED_BLOCK.register();
}

pub(crate) fn register_eth_to_strk_metrics() {
    ETH_TO_STRK_ERROR_COUNT.register();
    ETH_TO_STRK_SUCCESS_COUNT.register();
    ETH_TO_STRK_RATE.register();
}
