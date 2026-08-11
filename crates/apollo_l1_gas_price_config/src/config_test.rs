use std::collections::BTreeMap;
use std::mem::swap;

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
    FreshnessWindow,
    L1GasPriceProviderConfig,
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

/// One pair's sanity bounds, as `(minimum_micro_units, maximum_micro_units)`.
type SelectBounds = fn(&mut ChainlinkOracleConfig) -> (&mut u64, &mut u64);

/// Makes a pair's sanity bounds unusable.
type BreakBounds = fn(&mut u64, &mut u64);

fn eth_usd_bounds(config: &mut ChainlinkOracleConfig) -> (&mut u64, &mut u64) {
    (&mut config.eth_usd.minimum_micro_units, &mut config.eth_usd.maximum_micro_units)
}

fn strk_usd_bounds(config: &mut ChainlinkOracleConfig) -> (&mut u64, &mut u64) {
    (&mut config.strk_usd.minimum_micro_units, &mut config.strk_usd.maximum_micro_units)
}

fn eth_to_fri_bounds(config: &mut ChainlinkOracleConfig) -> (&mut u64, &mut u64) {
    (&mut config.eth_to_fri.minimum_micro_units, &mut config.eth_to_fri.maximum_micro_units)
}

fn zero_minimum(minimum_micro_units: &mut u64, _maximum_micro_units: &mut u64) {
    *minimum_micro_units = 0;
}

fn invert_bounds(minimum_micro_units: &mut u64, maximum_micro_units: &mut u64) {
    swap(minimum_micro_units, maximum_micro_units);
}

fn equalize_bounds(minimum_micro_units: &mut u64, maximum_micro_units: &mut u64) {
    *maximum_micro_units = *minimum_micro_units;
}

// Every combination must fail at config load, not at runtime. The validator checks the three pairs
// in one loop, so each pair is covered on each failure mode.
#[rstest]
fn rejects_unusable_chainlink_sanity_bounds(
    #[values(eth_usd_bounds, strk_usd_bounds, eth_to_fri_bounds)] select_bounds: SelectBounds,
    #[values(zero_minimum, invert_bounds, equalize_bounds)] break_bounds: BreakBounds,
) {
    let mut config = ChainlinkOracleConfig::default();
    let (minimum_micro_units, maximum_micro_units) = select_bounds(&mut config);
    break_bounds(minimum_micro_units, maximum_micro_units);
    assert!(config.validate().is_err());
}

// A zero `max_staleness_seconds` halts pricing, by rejecting every reading not written in the
// block's own second. A zero `failure_retry_interval_seconds` re-queries the feed on every
// proposal for as long as it keeps failing.
#[rstest]
#[case::zero_max_staleness(ChainlinkOracleConfig {
    freshness: FreshnessWindow {
        max_staleness_seconds: 0,
        ..ChainlinkOracleConfig::default().freshness
    },
    ..Default::default()
})]
#[case::zero_failure_retry_interval(ChainlinkOracleConfig {
    failure_retry_interval_seconds: 0,
    ..Default::default()
})]
fn rejects_out_of_range_chainlink_fields(#[case] config: ChainlinkOracleConfig) {
    assert!(config.validate().is_err());
}

// The default window's two bounds, pinned by direction rather than by name, so exchanging them in
// the `Default` literal fails here instead of only where a test happens to depend on their sizes.
#[test]
fn default_freshness_window_reaches_further_back_than_forward() {
    let freshness = ChainlinkOracleConfig::default().freshness;

    assert_eq!(freshness.max_staleness_seconds, (24 + 1) * 3600);
    assert_eq!(freshness.max_future_updated_at_seconds, 300);
}

#[rstest]
#[case::inverted(FreshnessWindow {
    max_staleness_seconds: 300,
    max_future_updated_at_seconds: 90_000,
})]
#[case::equal(FreshnessWindow { max_staleness_seconds: 300, max_future_updated_at_seconds: 300 })]
fn rejects_a_forward_bound_at_or_above_the_backward_bound(#[case] freshness: FreshnessWindow) {
    let config = ChainlinkOracleConfig { freshness, ..Default::default() };

    assert!(config.validate().is_err());
}

// The validator's single loop must cover all three pairs, not just the two read from a feed.
#[rstest]
#[case::eth_usd(|config: &mut ChainlinkOracleConfig| config.eth_usd.minimum_micro_units = 0)]
#[case::strk_usd(|config: &mut ChainlinkOracleConfig| config.strk_usd.minimum_micro_units = 0)]
#[case::eth_to_fri(|config: &mut ChainlinkOracleConfig| config.eth_to_fri.minimum_micro_units = 0)]
fn rejects_a_zero_minimum_on_every_pair(#[case] zero_the_minimum: fn(&mut ChainlinkOracleConfig)) {
    let mut config = ChainlinkOracleConfig::default();
    zero_the_minimum(&mut config);
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
