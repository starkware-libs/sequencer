use rstest::rstest;
use validator::Validate;

use super::{ChainlinkOracleConfig, L1GasPriceProviderConfig, MicroUnitBounds};

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

// `ChainlinkOracleClient::new` converts `lag_interval_seconds` via `expect`, so a dropped `range`
// attribute would panic at startup rather than fail config load. A zero `max_staleness_seconds`
// halts pricing the same way, by rejecting every reading not written in the queried second.
#[rstest]
#[case::zero_max_staleness(ChainlinkOracleConfig {
    max_staleness_seconds: 0,
    ..Default::default()
})]
#[case::zero_lag_interval(ChainlinkOracleConfig {
    lag_interval_seconds: 0,
    ..Default::default()
})]
fn rejects_out_of_range_chainlink_fields(#[case] config: ChainlinkOracleConfig) {
    assert!(config.validate().is_err());
}
