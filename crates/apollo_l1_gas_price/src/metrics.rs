use std::sync::Once;

use apollo_infra::metrics::{
    InfraMetrics,
    LocalClientMetrics,
    LocalServerMetrics,
    RemoteClientMetrics,
    RemoteServerMetrics,
};
use apollo_l1_gas_price_types::L1_GAS_PRICE_REQUEST_LABELS;
use apollo_metrics::metrics::{MetricCounter, MetricDetails, MetricGauge};
use apollo_metrics::{define_infra_metrics, define_metrics};

define_infra_metrics!(l1_gas_price);

define_metrics!(
    L1GasPrice => {
        MetricCounter { L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY, "l1_gas_price_provider_insufficient_history", "Number of times the L1 gas price provider calculated an average with too few blocks", init=0 },
        MetricCounter { L1_GAS_PRICE_SCRAPER_SUCCESS_COUNT, "l1_gas_price_scraper_success_count", "Number of times the L1 gas price scraper successfully scraped and updated gas prices", init=0 },
        MetricCounter { L1_GAS_PRICE_SCRAPER_BASELAYER_ERROR_COUNT, "l1_gas_price_scraper_baselayer_error_count", "Number of times the L1 gas price scraper encountered an error while scraping the base layer", init=0 },
        MetricCounter { L1_GAS_PRICE_SCRAPER_REORG_DETECTED, "l1_gas_price_scraper_reorg_detected", "Number of times the L1 gas price scraper detected a reorganization in the base layer", init=0 },
        MetricCounter { ETH_TO_STRK_ERROR_COUNT, "eth_to_strk_error_count", "Number of times the query to the Eth to Strk oracle failed due to an error or timeout", init=0 },
        MetricCounter { ETH_TO_STRK_SUCCESS_COUNT, "eth_to_strk_success_count", "Number of times the query to the Eth to Strk oracle succeeded", init=0 },
        MetricCounter { SNIP35_STRK_USD_ERROR_COUNT, "snip35_strk_usd_error_count", "Number of times the query to the STRK to USD oracle failed due to an error or timeout", init=0 },
        MetricCounter { SNIP35_STRK_USD_SUCCESS_COUNT, "snip35_strk_usd_success_count", "Number of times the query to the STRK to USD oracle succeeded", init=0 },
        MetricCounter { CHAINLINK_ORACLE_STALE_FEED_COUNT, "chainlink_oracle_stale_feed_count", "Number of times a Chainlink price feed reading was rejected because its update timestamp fell outside the accepted freshness window, either too old or too far in the future", init=0 },
        MetricCounter { CHAINLINK_ORACLE_RATE_OUT_OF_BOUNDS_COUNT, "chainlink_oracle_rate_out_of_bounds_count", "Number of times a Chainlink rate was rejected because it fell outside the configured absolute sanity bounds", init=0 },
        MetricCounter { CHAINLINK_ORACLE_INVALID_FEED_ANSWER_COUNT, "chainlink_oracle_invalid_feed_answer_count", "Number of times a Chainlink feed returned a zero answer or a decimals value outside the accepted range", init=0 },
        MetricCounter { CHAINLINK_ORACLE_CONTRACT_CALL_ERROR_COUNT, "chainlink_oracle_contract_call_error_count", "Number of times a Chainlink feed call to the batcher failed or returned undecodable retdata", init=0 },
        MetricGauge { L1_GAS_PRICE_SCRAPER_LAST_SUCCESS_TIMESTAMP_SECONDS, "l1_gas_price_scraper_last_success_timestamp_seconds", "Unix timestamp (seconds) of the last successful L1 gas price scrape" },
        MetricGauge { ETH_TO_STRK_LAST_SUCCESS_TIMESTAMP_SECONDS, "eth_to_strk_last_success_timestamp_seconds", "Unix timestamp (seconds) of the last successful ETH→STRK oracle query" },
        MetricGauge { SNIP35_STRK_USD_LAST_SUCCESS_TIMESTAMP_SECONDS, "snip35_strk_usd_last_success_timestamp_seconds", "Unix timestamp (seconds) of the last successful STRK→USD oracle query" },
        MetricGauge { L1_GAS_PRICE_SCRAPER_LATEST_SCRAPED_BLOCK, "l1_gas_price_scraper_latest_scraped_block", "The latest block number that the L1 gas price scraper has scraped" },
        MetricGauge { ETH_TO_STRK_RATE, "eth_to_strk_rate", "The current rate of ETH to STRK conversion" },
        MetricGauge { SNIP35_STRK_USD_RATE, "snip35_strk_usd_rate", "The STRK/USD rate from the oracle" },
        MetricGauge { L1_GAS_PRICE_LATEST_MEAN_VALUE, "l1_gas_price_latest_mean_value", "The latest L1 gas price, calculated as an average by the provider client" },
        MetricGauge { L1_DATA_GAS_PRICE_LATEST_MEAN_VALUE, "l1_data_gas_price_latest_mean_value", "The latest L1 data gas price, calculated as an average by the provider client" }
    },
);

/// Per-pair metric handles owned by an `ExchangeRateOracleClient`.
/// Each constructed client uses its own set so concurrent ETH→STRK and
/// STRK→USD clients update disjoint Prometheus series.
#[derive(Copy, Clone)]
pub struct ExchangeRateOracleMetrics {
    pub rate: &'static MetricGauge,
    pub success_count: &'static MetricCounter,
    pub error_count: &'static MetricCounter,
    pub last_success_timestamp: &'static MetricGauge,
    /// Guards this set's registration. Private so that every set is one of the constants below,
    /// each of which owns a distinct guard.
    registration_guard: &'static Once,
}

impl ExchangeRateOracleMetrics {
    /// Runs once per process per metric set: `register` resets each metric to its initial value
    /// rather than merely describing it, so two clients serving the same pair would otherwise wipe
    /// each other's counts depending on construction order.
    pub fn register(&self) {
        self.registration_guard.call_once(|| {
            self.rate.register();
            self.success_count.register();
            self.error_count.register();
            self.last_success_timestamp.register();
        });
    }
}

// Manual impl: `MetricGauge` / `MetricCounter` do not derive `Debug`,
// but the surrounding `ExchangeRateOracleClient` does. Printing the prom
// name of each metric is the only useful thing to surface.
impl std::fmt::Debug for ExchangeRateOracleMetrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExchangeRateOracleMetrics")
            .field("rate", &self.rate.get_name())
            .field("success_count", &self.success_count.get_name())
            .field("error_count", &self.error_count.get_name())
            .field("last_success_timestamp", &self.last_success_timestamp.get_name())
            .finish_non_exhaustive()
    }
}

static ETH_TO_STRK_REGISTRATION: Once = Once::new();
static STRK_TO_USD_REGISTRATION: Once = Once::new();

pub const ETH_TO_STRK_ORACLE_METRICS: ExchangeRateOracleMetrics = ExchangeRateOracleMetrics {
    rate: &ETH_TO_STRK_RATE,
    success_count: &ETH_TO_STRK_SUCCESS_COUNT,
    error_count: &ETH_TO_STRK_ERROR_COUNT,
    last_success_timestamp: &ETH_TO_STRK_LAST_SUCCESS_TIMESTAMP_SECONDS,
    registration_guard: &ETH_TO_STRK_REGISTRATION,
};

pub const STRK_TO_USD_ORACLE_METRICS: ExchangeRateOracleMetrics = ExchangeRateOracleMetrics {
    rate: &SNIP35_STRK_USD_RATE,
    success_count: &SNIP35_STRK_USD_SUCCESS_COUNT,
    error_count: &SNIP35_STRK_USD_ERROR_COUNT,
    last_success_timestamp: &SNIP35_STRK_USD_LAST_SUCCESS_TIMESTAMP_SECONDS,
    registration_guard: &STRK_TO_USD_REGISTRATION,
};

pub(crate) fn register_provider_metrics() {
    L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY.register();
    L1_GAS_PRICE_LATEST_MEAN_VALUE.register();
    L1_DATA_GAS_PRICE_LATEST_MEAN_VALUE.register();
}

/// Guard-trip counters shared by all `ChainlinkOracleClient` instances. They record *why* a query
/// was rejected; the per-pair `ExchangeRateOracleMetrics::error_count` records *which* pair.
///
/// Registered once per process, for the same reason as `ExchangeRateOracleMetrics::register`.
pub(crate) fn register_chainlink_guard_metrics() {
    static REGISTER_GUARD_METRICS: Once = Once::new();
    REGISTER_GUARD_METRICS.call_once(|| {
        CHAINLINK_ORACLE_STALE_FEED_COUNT.register();
        CHAINLINK_ORACLE_RATE_OUT_OF_BOUNDS_COUNT.register();
        CHAINLINK_ORACLE_INVALID_FEED_ANSWER_COUNT.register();
        CHAINLINK_ORACLE_CONTRACT_CALL_ERROR_COUNT.register();
    });
}

pub(crate) fn register_scraper_metrics() {
    L1_GAS_PRICE_SCRAPER_SUCCESS_COUNT.register();
    L1_GAS_PRICE_SCRAPER_BASELAYER_ERROR_COUNT.register();
    L1_GAS_PRICE_SCRAPER_REORG_DETECTED.register();
    L1_GAS_PRICE_SCRAPER_LAST_SUCCESS_TIMESTAMP_SECONDS.register();
    L1_GAS_PRICE_SCRAPER_LATEST_SCRAPED_BLOCK.register();
}
