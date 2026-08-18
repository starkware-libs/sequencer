use apollo_l1_gas_price_types::CurrencyPair;
use metrics_exporter_prometheus::PrometheusBuilder;
use strum::IntoEnumIterator;

use super::{
    register_chainlink_guard_metrics,
    CHAINLINK_ORACLE_CONTRACT_CALL_ERROR_COUNT,
    CHAINLINK_ORACLE_FUTURE_FEED_COUNT,
    CHAINLINK_ORACLE_INVALID_FEED_ANSWER_COUNT,
    CHAINLINK_ORACLE_RATE_OUT_OF_BOUNDS_COUNT,
    CHAINLINK_ORACLE_STALE_FEED_COUNT,
};

/// Registration publishes a zero sample per label permutation, so a guard that never trips renders
/// as 0 instead of being absent from the scrape.
#[test]
fn chainlink_guard_metrics_register_at_zero_for_every_pair() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);

    register_chainlink_guard_metrics();

    let metrics_as_string = recorder.handle().render();
    for currency_pair in CurrencyPair::iter() {
        let labels = currency_pair.labels();
        CHAINLINK_ORACLE_STALE_FEED_COUNT.assert_eq::<u64>(&metrics_as_string, 0, &labels);
        CHAINLINK_ORACLE_FUTURE_FEED_COUNT.assert_eq::<u64>(&metrics_as_string, 0, &labels);
        CHAINLINK_ORACLE_RATE_OUT_OF_BOUNDS_COUNT.assert_eq::<u64>(&metrics_as_string, 0, &labels);
        CHAINLINK_ORACLE_INVALID_FEED_ANSWER_COUNT.assert_eq::<u64>(&metrics_as_string, 0, &labels);
        CHAINLINK_ORACLE_CONTRACT_CALL_ERROR_COUNT.assert_eq::<u64>(&metrics_as_string, 0, &labels);
    }
}
