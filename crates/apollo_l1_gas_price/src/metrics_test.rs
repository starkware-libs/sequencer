use std::collections::BTreeMap;
use std::sync::Once;

use apollo_config::converters::UrlAndHeaders;
use apollo_l1_gas_price_config::config::ExchangeRateOracleConfig;
use apollo_l1_gas_price_types::errors::ExchangeRateOracleErrorType;
use apollo_l1_gas_price_types::{CurrencyPair, LABEL_NAME_CURRENCY_PAIR, LABEL_NAME_ERROR_TYPE};
use metrics_exporter_prometheus::PrometheusBuilder;
use strum::IntoEnumIterator;
use url::Url;

use super::{ExchangeRateOracleMetrics, ETH_TO_STRK_ORACLE_METRICS};
use crate::exchange_rate_oracle::ExchangeRateOracleClient;

/// Guards owned by the tests, so the assertions on their state do not depend on whether another
/// test in the process already registered the shared oracle metrics.
static REPEATED_CONSTRUCTION_REGISTRATION: Once = Once::new();
static ZERO_SAMPLE_REGISTRATION: Once = Once::new();

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
        registration_guard: &REPEATED_CONSTRUCTION_REGISTRATION,
        ..ETH_TO_STRK_ORACLE_METRICS
    };
    assert!(!REPEATED_CONSTRUCTION_REGISTRATION.is_completed());

    let _first_client = ExchangeRateOracleClient::new(oracle_config(), oracle_metrics);
    assert!(REPEATED_CONSTRUCTION_REGISTRATION.is_completed());

    oracle_metrics.record_success(5);
    let _second_client = ExchangeRateOracleClient::new(oracle_config(), oracle_metrics);

    oracle_metrics.success_count.assert_eq::<u64>(
        &recorder.handle().render(),
        1,
        &CurrencyPair::EthStrk.labels(),
    );
}

/// Registration publishes a zero sample per label permutation, so a pair that never failed renders
/// as 0 instead of being absent from the scrape.
#[test]
fn oracle_metrics_register_at_zero_for_every_permutation() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);

    let oracle_metrics = ExchangeRateOracleMetrics {
        registration_guard: &ZERO_SAMPLE_REGISTRATION,
        ..ETH_TO_STRK_ORACLE_METRICS
    };
    oracle_metrics.register();

    let metrics_as_string = recorder.handle().render();
    for currency_pair in CurrencyPair::iter() {
        oracle_metrics.success_count.assert_eq::<u64>(
            &metrics_as_string,
            0,
            &currency_pair.labels(),
        );
        for error_type in ExchangeRateOracleErrorType::iter() {
            oracle_metrics.error_count.assert_eq::<u64>(
                &metrics_as_string,
                0,
                &[
                    (LABEL_NAME_CURRENCY_PAIR, <&str>::from(currency_pair)),
                    (LABEL_NAME_ERROR_TYPE, <&str>::from(error_type)),
                ],
            );
        }
    }
}
