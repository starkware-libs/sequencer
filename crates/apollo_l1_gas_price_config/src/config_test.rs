use std::collections::BTreeMap;

use apollo_config::dumping::SerializeConfig;
use apollo_config::loading::load;
use apollo_config::{ParamPath, SerializedContent};
use rstest::rstest;
use serde_json::{json, Value};
use validator::Validate;

use super::{ChainlinkOracleConfig, ExchangeRateOracleSource, L1GasPriceProviderConfig};

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

// A zero minimum accepts any answer down to 1, silently disabling the lower sanity bound, and an
// inverted pair rejects every reading forever. Both must fail at config load, not at runtime.
#[rstest]
#[case::zero_eth_usd_minimum(ChainlinkOracleConfig {
    min_eth_usd_price_micro_usd: 0,
    ..Default::default()
})]
#[case::zero_strk_usd_minimum(ChainlinkOracleConfig {
    min_strk_usd_price_micro_usd: 0,
    ..Default::default()
})]
#[case::zero_eth_to_fri_minimum(ChainlinkOracleConfig {
    min_eth_to_fri_rate_micro_strk: 0,
    ..Default::default()
})]
#[case::inverted_eth_usd_bounds(ChainlinkOracleConfig {
    min_eth_usd_price_micro_usd: ChainlinkOracleConfig::default().max_eth_usd_price_micro_usd,
    max_eth_usd_price_micro_usd: ChainlinkOracleConfig::default().min_eth_usd_price_micro_usd,
    ..Default::default()
})]
#[case::equal_strk_usd_bounds(ChainlinkOracleConfig {
    max_strk_usd_price_micro_usd: ChainlinkOracleConfig::default().min_strk_usd_price_micro_usd,
    ..Default::default()
})]
#[case::inverted_eth_to_fri_bounds(ChainlinkOracleConfig {
    min_eth_to_fri_rate_micro_strk: ChainlinkOracleConfig::default().max_eth_to_fri_rate_micro_strk,
    max_eth_to_fri_rate_micro_strk: ChainlinkOracleConfig::default().min_eth_to_fri_rate_micro_strk,
    ..Default::default()
})]
fn rejects_unusable_chainlink_sanity_bounds(#[case] config: ChainlinkOracleConfig) {
    assert!(config.validate().is_err());
}

// The queried timestamp trails the block timestamp by up to two lag intervals, so a tolerance at
// or below that would reject rounds written in the block currently being built.
#[rstest]
#[case::exactly_twice_the_lag_interval(120, false)]
#[case::just_above_twice_the_lag_interval(121, true)]
fn future_tolerance_must_exceed_twice_the_lag_interval(
    #[case] max_future_updated_at_seconds: u64,
    #[case] is_accepted: bool,
) {
    let config = ChainlinkOracleConfig {
        lag_interval_seconds: 60,
        max_future_updated_at_seconds,
        ..Default::default()
    };
    assert_eq!(config.validate().is_ok(), is_accepted);
}

// `ChainlinkOracleClient::new` turns `lag_interval_seconds` and `max_cache_size` into non-zero
// types with an `expect`, so a dropped `range` attribute would turn a config error into a node
// panic at startup. A zero `max_staleness_seconds` rejects every reading that is not written in
// the queried second, which halts pricing just as effectively.
#[rstest]
#[case::zero_max_staleness(ChainlinkOracleConfig {
    max_staleness_seconds: 0,
    ..Default::default()
})]
#[case::zero_lag_interval(ChainlinkOracleConfig {
    lag_interval_seconds: 0,
    ..Default::default()
})]
#[case::zero_max_cache_size(ChainlinkOracleConfig {
    max_cache_size: 0,
    ..Default::default()
})]
fn rejects_zero_valued_chainlink_fields(#[case] config: ChainlinkOracleConfig) {
    assert!(config.validate().is_err());
}

// The Chainlink config is only reachable through the provider config, so the checks above are
// vacuous unless the nesting carries the validation through.
#[test]
fn provider_validation_reaches_the_nested_chainlink_config() {
    let config = L1GasPriceProviderConfig {
        chainlink_oracle_config: ChainlinkOracleConfig { max_cache_size: 0, ..Default::default() },
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
