use std::collections::BTreeMap;

use apollo_config::dumping::SerializeConfig;
use apollo_config::loading::load;
use apollo_config::{ParamPath, SerializedContent};
use rstest::rstest;
use serde_json::{json, Value};
use validator::Validate;

use super::{
    ChainlinkOracleConfig,
    ExchangeRateOracleConfig,
    ExchangeRateOracleSource,
    L1GasPriceProviderConfig,
    MicroUnitBounds,
};

// A zero mean window would make the provider divide by zero when computing the mean gas price,
// so it must be rejected at config load instead of panicking later during block production.
#[test]
fn rejects_zero_number_of_blocks_for_mean() {
    let config = L1GasPriceProviderConfig { number_of_blocks_for_mean: 0, ..Default::default() };
    assert!(config.validate().is_err());
}

#[test]
fn accepts_default_number_of_blocks_for_mean() {
    assert!(L1GasPriceProviderConfig::default().validate().is_ok());
}

#[test]
fn accepts_default_chainlink_oracle_config() {
    assert!(ChainlinkOracleConfig::default().validate().is_ok());
}

fn zeroed_minimum(bounds: MicroUnitBounds) -> MicroUnitBounds {
    MicroUnitBounds { minimum_micro_units: 0, ..bounds }
}

fn inverted(bounds: MicroUnitBounds) -> MicroUnitBounds {
    MicroUnitBounds {
        minimum_micro_units: bounds.maximum_micro_units,
        maximum_micro_units: bounds.minimum_micro_units,
    }
}

// Both cases must fail at config load, not at runtime.
#[rstest]
#[case::zero_eth_usd_minimum(ChainlinkOracleConfig {
    eth_usd_price_bounds: zeroed_minimum(ChainlinkOracleConfig::default().eth_usd_price_bounds),
    ..Default::default()
})]
#[case::zero_strk_usd_minimum(ChainlinkOracleConfig {
    strk_usd_price_bounds: zeroed_minimum(ChainlinkOracleConfig::default().strk_usd_price_bounds),
    ..Default::default()
})]
#[case::zero_eth_to_fri_minimum(ChainlinkOracleConfig {
    eth_to_fri_rate_bounds: zeroed_minimum(ChainlinkOracleConfig::default().eth_to_fri_rate_bounds),
    ..Default::default()
})]
#[case::inverted_eth_usd_bounds(ChainlinkOracleConfig {
    eth_usd_price_bounds: inverted(ChainlinkOracleConfig::default().eth_usd_price_bounds),
    ..Default::default()
})]
#[case::equal_strk_usd_bounds(ChainlinkOracleConfig {
    strk_usd_price_bounds: MicroUnitBounds {
        maximum_micro_units:
            ChainlinkOracleConfig::default().strk_usd_price_bounds.minimum_micro_units,
        ..ChainlinkOracleConfig::default().strk_usd_price_bounds
    },
    ..Default::default()
})]
#[case::inverted_eth_to_fri_bounds(ChainlinkOracleConfig {
    eth_to_fri_rate_bounds: inverted(ChainlinkOracleConfig::default().eth_to_fri_rate_bounds),
    ..Default::default()
})]
fn rejects_unusable_chainlink_sanity_bounds(#[case] config: ChainlinkOracleConfig) {
    assert!(config.validate().is_err());
}

// A zero `max_staleness_seconds` halts pricing, by rejecting every reading not written in the
// block's own second. A zero `failure_retry_interval_seconds` re-queries the feed on every
// proposal for as long as it keeps failing.
#[rstest]
#[case::zero_max_staleness(ChainlinkOracleConfig {
    max_staleness_seconds: 0,
    ..Default::default()
})]
#[case::zero_failure_retry_interval(ChainlinkOracleConfig {
    failure_retry_interval_seconds: 0,
    ..Default::default()
})]
fn rejects_out_of_range_chainlink_fields(#[case] config: ChainlinkOracleConfig) {
    assert!(config.validate().is_err());
}

// `build_exchange_rate_oracle_client` converts this feed's `lag_interval_seconds` into the
// Chainlink client's sampling interval via `expect`, so a dropped `range` attribute would panic at
// startup rather than fail config load.
#[rstest]
#[case::eth_to_strk_feed(true, false)]
#[case::strk_to_usd_feed(false, true)]
fn rejects_zero_lag_interval_on_either_feed(
    #[case] is_eth_to_strk_zeroed: bool,
    #[case] is_strk_to_usd_zeroed: bool,
) {
    let zeroed_lag_interval =
        ExchangeRateOracleConfig { lag_interval_seconds: 0, ..Default::default() };
    let mut config = L1GasPriceProviderConfig::default();
    if is_eth_to_strk_zeroed {
        config.eth_to_strk_oracle_config = zeroed_lag_interval.clone();
    }
    if is_strk_to_usd_zeroed {
        config.strk_to_usd_oracle_config = zeroed_lag_interval;
    }
    assert!(config.validate().is_err());
}

// The Chainlink config is only reachable through the provider config, so the checks above are
// vacuous unless the nesting carries the validation through.
#[test]
fn provider_validation_reaches_the_nested_chainlink_config() {
    let config = L1GasPriceProviderConfig {
        chainlink_oracle_config: ChainlinkOracleConfig {
            failure_retry_interval_seconds: 0,
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(config.validate().is_err());
}

#[test]
fn default_oracle_sources_are_http() {
    let config = L1GasPriceProviderConfig::default();
    assert_eq!(config.eth_to_strk_oracle_source, ExchangeRateOracleSource::Http);
    assert_eq!(config.strk_to_usd_oracle_source, ExchangeRateOracleSource::Http);
}

// Each feed is switched on its own, so all four combinations must survive a dump and load.
#[rstest]
#[case::both_http(ExchangeRateOracleSource::Http, ExchangeRateOracleSource::Http)]
#[case::eth_to_strk_only(ExchangeRateOracleSource::Chainlink, ExchangeRateOracleSource::Http)]
#[case::strk_to_usd_only(ExchangeRateOracleSource::Http, ExchangeRateOracleSource::Chainlink)]
#[case::both_chainlink(ExchangeRateOracleSource::Chainlink, ExchangeRateOracleSource::Chainlink)]
fn oracle_sources_round_trip_through_serialize_config(
    #[case] eth_to_strk_oracle_source: ExchangeRateOracleSource,
    #[case] strk_to_usd_oracle_source: ExchangeRateOracleSource,
) {
    let config = L1GasPriceProviderConfig {
        eth_to_strk_oracle_source,
        strk_to_usd_oracle_source,
        ..Default::default()
    };
    assert_eq!(load::<L1GasPriceProviderConfig>(&dumped_values(&config)).unwrap(), config);
}

// The enum takes no `serde` renaming, so the variant names are matched exactly as written in Rust.
// An operator who types the source in the casing used everywhere else in the config file must be
// told, rather than silently left on the `Http` default.
#[rstest]
#[case::lowercase("chainlink")]
#[case::uppercase("CHAINLINK")]
#[case::snake_case("eth_to_strk")]
#[case::unknown_source("Coinbase")]
fn unrecognized_oracle_source_fails_to_load(#[case] source_value: &str) {
    for param_path in ["eth_to_strk_oracle_source", "strk_to_usd_oracle_source"] {
        let mut config_values = dumped_values(&L1GasPriceProviderConfig::default());
        config_values.insert(param_path.to_string(), json!(source_value));
        assert!(
            load::<L1GasPriceProviderConfig>(&config_values).is_err(),
            "{param_path} accepted the unrecognized value {source_value}"
        );
    }
}

/// The config as an operator's config file holds it: one flat map from param path to value.
fn dumped_values(config: &L1GasPriceProviderConfig) -> BTreeMap<ParamPath, Value> {
    config
        .dump()
        .into_iter()
        .map(|(param_path, param)| match param.content {
            SerializedContent::DefaultValue(value) => (param_path, value),
            content => panic!("Expected a default value for {param_path}, got {content:?}"),
        })
        .collect()
}
