use std::collections::BTreeMap;
use std::sync::Once;

use apollo_config::converters::UrlAndHeaders;
use apollo_l1_gas_price_config::config::ExchangeRateOracleConfig;
use apollo_l1_gas_price_types::CurrencyPair;
use metrics_exporter_prometheus::PrometheusBuilder;
use strum::IntoEnumIterator;
use url::Url;

use super::{
    register_chainlink_guard_metrics,
    ExchangeRateOracleMetrics,
    CHAINLINK_ORACLE_CONTRACT_CALL_ERROR_COUNT,
    CHAINLINK_ORACLE_FUTURE_FEED_COUNT,
    CHAINLINK_ORACLE_INVALID_FEED_ANSWER_COUNT,
    CHAINLINK_ORACLE_RATE_OUT_OF_BOUNDS_COUNT,
    CHAINLINK_ORACLE_STALE_FEED_COUNT,
    ETH_TO_STRK_ORACLE_METRICS,
};
use crate::exchange_rate_oracle::ExchangeRateOracleClient;

/// Guard owned by this test, so the assertions on its state do not depend on whether another test
/// in the process already registered the ETH→STRK set.
static TEST_REGISTRATION: Once = Once::new();

fn oracle_config() -> ExchangeRateOracleConfig {
    let url_and_headers =
        UrlAndHeaders { url: Url::parse("http://localhost:1").unwrap(), headers: BTreeMap::new() };
    ExchangeRateOracleConfig {
        url_header_list: Some(vec![url_and_headers.into()]),
        ..Default::default()
    }
}

/// Models `create_node_modules` running once per simulated node inside a single flow-test process
/// (`apollo_integration_tests/src/flow_test_setup.rs:336`): each node builds its own client for the
/// same pair against the same static metric set, so a later construction must not re-register the
/// set or reset the counts recorded for an earlier one.
#[test]
fn repeated_client_construction_registers_metrics_once() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);

    let oracle_metrics = ExchangeRateOracleMetrics {
        registration_guard: &TEST_REGISTRATION,
        ..ETH_TO_STRK_ORACLE_METRICS
    };
    assert!(!TEST_REGISTRATION.is_completed());

    let _first_client = ExchangeRateOracleClient::new(oracle_config(), oracle_metrics);
    assert!(TEST_REGISTRATION.is_completed());

    oracle_metrics.success_count.increment(1);
    let _second_client = ExchangeRateOracleClient::new(oracle_config(), oracle_metrics);

    oracle_metrics.success_count.assert_eq::<u64>(&recorder.handle().render(), 1);
}

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
