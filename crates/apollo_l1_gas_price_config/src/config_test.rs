use std::mem::swap;

use rstest::rstest;
use validator::Validate;

use super::{L1GasPriceProviderConfig, RateBoundsConfig};

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
fn accepts_default_rate_bounds_config() {
    assert!(RateBoundsConfig::default().validate().is_ok());
}

/// One pair's bounds, as `(minimum_micro_units, maximum_micro_units)`.
type SelectBounds = fn(&mut RateBoundsConfig) -> (&mut u64, &mut u64);

/// Makes a pair's bounds unusable.
type BreakBounds = fn(&mut u64, &mut u64);

fn select_eth_usd_bounds(config: &mut RateBoundsConfig) -> (&mut u64, &mut u64) {
    (&mut config.eth_usd.minimum_micro_units, &mut config.eth_usd.maximum_micro_units)
}

fn select_strk_usd_bounds(config: &mut RateBoundsConfig) -> (&mut u64, &mut u64) {
    (&mut config.strk_usd.minimum_micro_units, &mut config.strk_usd.maximum_micro_units)
}

fn select_eth_to_fri_bounds(config: &mut RateBoundsConfig) -> (&mut u64, &mut u64) {
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

// Every pair and failure mode must be rejected at config load.
#[rstest]
fn rejects_unusable_rate_bounds(
    #[values(select_eth_usd_bounds, select_strk_usd_bounds, select_eth_to_fri_bounds)]
    select_bounds: SelectBounds,
    #[values(zero_minimum, invert_bounds, equalize_bounds)] break_bounds: BreakBounds,
) {
    let mut config = RateBoundsConfig::default();
    let (minimum_micro_units, maximum_micro_units) = select_bounds(&mut config);
    break_bounds(minimum_micro_units, maximum_micro_units);
    assert!(config.validate().is_err());
}
