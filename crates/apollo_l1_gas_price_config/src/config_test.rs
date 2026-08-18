use std::mem::swap;

use rstest::rstest;
use validator::Validate;

use super::{
    AllRateBoundsConfig,
    ChainlinkOracleConfig,
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
fn accepts_default_all_rate_bounds_config() {
    assert!(AllRateBoundsConfig::default().validate().is_ok());
}

/// One pair's bounds, as `(minimum_micro_units, maximum_micro_units)`.
type SelectBounds = fn(&mut AllRateBoundsConfig) -> (&mut u64, &mut u64);

/// Makes a pair's bounds unusable.
type BreakBounds = fn(&mut u64, &mut u64);

fn select_eth_usd_bounds(config: &mut AllRateBoundsConfig) -> (&mut u64, &mut u64) {
    (&mut config.eth_usd.minimum_micro_units, &mut config.eth_usd.maximum_micro_units)
}

fn select_strk_usd_bounds(config: &mut AllRateBoundsConfig) -> (&mut u64, &mut u64) {
    (&mut config.strk_usd.minimum_micro_units, &mut config.strk_usd.maximum_micro_units)
}

fn select_eth_strk_bounds(config: &mut AllRateBoundsConfig) -> (&mut u64, &mut u64) {
    (&mut config.eth_strk.minimum_micro_units, &mut config.eth_strk.maximum_micro_units)
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

// Every pair and failure mode must be rejected at config load.
#[rstest]
fn rejects_unusable_rate_bounds(
    #[values(select_eth_usd_bounds, select_strk_usd_bounds, select_eth_strk_bounds)]
    select_bounds: SelectBounds,
    #[values(zero_minimum, invert_bounds, equalize_bounds)] break_bounds: BreakBounds,
) {
    let mut config = AllRateBoundsConfig::default();
    let (minimum_micro_units, maximum_micro_units) = select_bounds(&mut config);
    break_bounds(minimum_micro_units, maximum_micro_units);
    assert!(config.validate().is_err());
}

#[test]
fn accepts_default_chainlink_oracle_config() {
    assert!(ChainlinkOracleConfig::default().validate().is_ok());
}

// A zero `max_staleness_seconds` rejects every reading not written in the block's own second;
// a zero `failure_retry_interval_seconds` re-queries a failing feed on every proposal.
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

// A zero `sampling_interval_seconds` re-reads the feeds on every proposal, so one second is the
// smallest accepted interval.
#[rstest]
#[case::zero(0, false)]
#[case::one_second(1, true)]
fn validates_sampling_interval_seconds_range(
    #[case] sampling_interval_seconds: u64,
    #[case] is_accepted: bool,
) {
    let config = ChainlinkOracleConfig { sampling_interval_seconds, ..Default::default() };

    assert_eq!(config.validate().is_ok(), is_accepted);
}

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
