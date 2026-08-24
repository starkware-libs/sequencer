use std::sync::Once;

use apollo_infra::metrics::{
    InfraMetrics,
    LocalClientMetrics,
    LocalServerMetrics,
    RemoteClientMetrics,
    RemoteServerMetrics,
};
use apollo_l1_gas_price_types::errors::ExchangeRateOracleErrorType;
use apollo_l1_gas_price_types::{
    CurrencyPair,
    ExchangeRate,
    L1_GAS_PRICE_REQUEST_LABELS,
    LABEL_NAME_CURRENCY_PAIR,
    LABEL_NAME_ERROR_TYPE,
};
use apollo_metrics::metrics::{
    set_unix_now_seconds_with_labels,
    LabeledMetricCounter,
    LabeledMetricGauge,
    MetricDetails,
};
use apollo_metrics::{define_infra_metrics, define_metrics, generate_permutation_labels};

#[cfg(test)]
#[path = "metrics_test.rs"]
mod metrics_test;

define_infra_metrics!(l1_gas_price);

generate_permutation_labels! {
    CURRENCY_PAIR_LABELS,
    (LABEL_NAME_CURRENCY_PAIR, CurrencyPair),
}

generate_permutation_labels! {
    CURRENCY_PAIR_AND_ERROR_TYPE_LABELS,
    (LABEL_NAME_CURRENCY_PAIR, CurrencyPair),
    (LABEL_NAME_ERROR_TYPE, ExchangeRateOracleErrorType),
}

define_metrics!(
    L1GasPrice => {
        MetricCounter { L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY, "l1_gas_price_provider_insufficient_history", "Number of times the L1 gas price provider calculated an average with too few blocks", init=0 },
        MetricCounter { L1_GAS_PRICE_SCRAPER_SUCCESS_COUNT, "l1_gas_price_scraper_success_count", "Number of times the L1 gas price scraper successfully scraped and updated gas prices", init=0 },
        MetricCounter { L1_GAS_PRICE_SCRAPER_BASELAYER_ERROR_COUNT, "l1_gas_price_scraper_baselayer_error_count", "Number of times the L1 gas price scraper encountered an error while scraping the base layer", init=0 },
        MetricCounter { L1_GAS_PRICE_SCRAPER_REORG_DETECTED, "l1_gas_price_scraper_reorg_detected", "Number of times the L1 gas price scraper detected a reorganization in the base layer", init=0 },
        MetricGauge { L1_GAS_PRICE_SCRAPER_LAST_SUCCESS_TIMESTAMP_SECONDS, "l1_gas_price_scraper_last_success_timestamp_seconds", "Unix timestamp (seconds) of the last successful L1 gas price scrape" },
        MetricGauge { L1_GAS_PRICE_SCRAPER_LATEST_SCRAPED_BLOCK, "l1_gas_price_scraper_latest_scraped_block", "The latest block number that the L1 gas price scraper has scraped" },
        MetricGauge { L1_GAS_PRICE_LATEST_MEAN_VALUE, "l1_gas_price_latest_mean_value", "The latest L1 gas price, calculated as an average by the provider client" },
        MetricGauge { L1_DATA_GAS_PRICE_LATEST_MEAN_VALUE, "l1_data_gas_price_latest_mean_value", "The latest L1 data gas price, calculated as an average by the provider client" },
        LabeledMetricCounter { EXCHANGE_RATE_ORACLE_SUCCESS_COUNT, "exchange_rate_oracle_success_count", "Number of times a query to the exchange rate oracle succeeded, per currency pair", init=0, labels = CURRENCY_PAIR_LABELS },
        LabeledMetricCounter { EXCHANGE_RATE_ORACLE_ERROR_COUNT, "exchange_rate_oracle_error_count", "Number of times a query to the exchange rate oracle failed, per currency pair and error type", init=0, labels = CURRENCY_PAIR_AND_ERROR_TYPE_LABELS },
        LabeledMetricGauge { EXCHANGE_RATE_ORACLE_LAST_SUCCESS_TIMESTAMP_SECONDS, "exchange_rate_oracle_last_success_timestamp_seconds", "Unix timestamp (seconds) of the last successful exchange rate oracle query, per currency pair", labels = CURRENCY_PAIR_LABELS },
        LabeledMetricGauge { EXCHANGE_RATE_ORACLE_RATE, "exchange_rate_oracle_rate", "The exchange rate the oracle last served, per currency pair", labels = CURRENCY_PAIR_LABELS }
    },
);

/// The oracle metrics a client records, bound to the pair it serves.
///
/// Every pair writes the same metrics and separates its series by the `currency_pair` label, so the
/// handles are identical in every instance and only `pair` differs.
#[derive(Copy, Clone)]
pub struct ExchangeRateOracleMetrics {
    rate: &'static LabeledMetricGauge,
    success_count: &'static LabeledMetricCounter,
    error_count: &'static LabeledMetricCounter,
    last_success_timestamp: &'static LabeledMetricGauge,
    pair: CurrencyPair,
    /// Guards registration of the metrics above, shared by every pair: registering a labeled
    /// metric writes every permutation, so a second registration would zero the counts already
    /// recorded for the other pairs.
    registration_guard: &'static Once,
}

impl ExchangeRateOracleMetrics {
    pub fn register(&self) {
        self.registration_guard.call_once(|| {
            self.rate.register();
            self.success_count.register();
            self.error_count.register();
            self.last_success_timestamp.register();
        });
    }

    /// Records a query that passed every guard: the rate served, when it resolved, and the success.
    pub fn record_success(&self, rate: ExchangeRate) {
        let pair_labels = self.pair.labels();
        self.success_count.increment(1, &pair_labels);
        set_unix_now_seconds_with_labels(self.last_success_timestamp, &pair_labels);
        self.rate.set_lossy(rate, &pair_labels);
    }

    pub fn record_error(&self, error_type: ExchangeRateOracleErrorType) {
        self.error_count.increment(
            1,
            &[
                (LABEL_NAME_CURRENCY_PAIR, self.pair.into()),
                (LABEL_NAME_ERROR_TYPE, error_type.into()),
            ],
        );
    }
}

// Manual impl: `LabeledMetricGauge` / `LabeledMetricCounter` do not derive `Debug`, but the
// surrounding `ExchangeRateOracleClient` does. The pair is what distinguishes one set from another.
impl std::fmt::Debug for ExchangeRateOracleMetrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExchangeRateOracleMetrics")
            .field("pair", &self.pair)
            .field("rate", &self.rate.get_name())
            .field("success_count", &self.success_count.get_name())
            .field("error_count", &self.error_count.get_name())
            .field("last_success_timestamp", &self.last_success_timestamp.get_name())
            .finish()
    }
}

static ORACLE_METRICS_REGISTRATION: Once = Once::new();

pub const ETH_TO_STRK_ORACLE_METRICS: ExchangeRateOracleMetrics = ExchangeRateOracleMetrics {
    rate: &EXCHANGE_RATE_ORACLE_RATE,
    success_count: &EXCHANGE_RATE_ORACLE_SUCCESS_COUNT,
    error_count: &EXCHANGE_RATE_ORACLE_ERROR_COUNT,
    last_success_timestamp: &EXCHANGE_RATE_ORACLE_LAST_SUCCESS_TIMESTAMP_SECONDS,
    pair: CurrencyPair::EthStrk,
    registration_guard: &ORACLE_METRICS_REGISTRATION,
};

pub const STRK_TO_USD_ORACLE_METRICS: ExchangeRateOracleMetrics = ExchangeRateOracleMetrics {
    rate: &EXCHANGE_RATE_ORACLE_RATE,
    success_count: &EXCHANGE_RATE_ORACLE_SUCCESS_COUNT,
    error_count: &EXCHANGE_RATE_ORACLE_ERROR_COUNT,
    last_success_timestamp: &EXCHANGE_RATE_ORACLE_LAST_SUCCESS_TIMESTAMP_SECONDS,
    pair: CurrencyPair::StrkUsd,
    registration_guard: &ORACLE_METRICS_REGISTRATION,
};

pub(crate) fn register_provider_metrics() {
    L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY.register();
    L1_GAS_PRICE_LATEST_MEAN_VALUE.register();
    L1_DATA_GAS_PRICE_LATEST_MEAN_VALUE.register();
}

pub(crate) fn register_scraper_metrics() {
    L1_GAS_PRICE_SCRAPER_SUCCESS_COUNT.register();
    L1_GAS_PRICE_SCRAPER_BASELAYER_ERROR_COUNT.register();
    L1_GAS_PRICE_SCRAPER_REORG_DETECTED.register();
    L1_GAS_PRICE_SCRAPER_LAST_SUCCESS_TIMESTAMP_SECONDS.register();
    L1_GAS_PRICE_SCRAPER_LATEST_SCRAPED_BLOCK.register();
}
