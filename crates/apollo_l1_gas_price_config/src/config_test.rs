use std::mem::swap;

use apollo_config::dumping::SerializeConfig;
use apollo_l1_gas_price_types::CurrencyPair;
use rstest::rstest;
use validator::Validate;

use super::{L1GasPriceProviderConfig, RateBounds, RateBoundsConfig};

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

// A mismatched arm would judge a rate against another pair's bounds, which the per-pair defaults
// being distinct catches.
#[test]
fn bounds_belong_to_their_pair() {
    let config = RateBoundsConfig::default();
    assert_eq!(config.bounds(CurrencyPair::EthUsd), config.eth_usd);
    assert_eq!(config.bounds(CurrencyPair::StrkUsd), config.strk_usd);
    assert_eq!(config.bounds(CurrencyPair::EthStrk), config.eth_strk);
}

// A dumped param path that misses a field name loads that pair's bounds from the schema default
// instead of the operator's value, so the paths are taken from serde rather than hardcoded.
#[test]
fn dumps_bounds_under_their_serde_field_names() {
    let config = RateBoundsConfig::default();
    let dump = config.dump();
    let serialized = serde_json::to_value(&config).unwrap();
    for field_name in serialized.as_object().unwrap().keys() {
        for bound_name in ["minimum_micro_units", "maximum_micro_units"] {
            let param_path = format!("{field_name}.{bound_name}");
            assert!(dump.contains_key(&param_path), "{param_path} is missing from the dump");
        }
    }
}

/// Makes a pair's bounds unusable.
type BreakBounds = fn(&mut RateBounds);

fn bounds_mut(config: &mut RateBoundsConfig, pair: CurrencyPair) -> &mut RateBounds {
    match pair {
        CurrencyPair::EthUsd => &mut config.eth_usd,
        CurrencyPair::StrkUsd => &mut config.strk_usd,
        CurrencyPair::EthStrk => &mut config.eth_strk,
    }
}

fn zero_minimum(bounds: &mut RateBounds) {
    bounds.minimum_micro_units = 0;
}

fn invert_bounds(bounds: &mut RateBounds) {
    swap(&mut bounds.minimum_micro_units, &mut bounds.maximum_micro_units);
}

fn equalize_bounds(bounds: &mut RateBounds) {
    bounds.maximum_micro_units = bounds.minimum_micro_units;
}

// Every pair and failure mode must be rejected at config load.
#[rstest]
fn rejects_unusable_rate_bounds(
    #[values(CurrencyPair::EthUsd, CurrencyPair::StrkUsd, CurrencyPair::EthStrk)]
    pair: CurrencyPair,
    #[values(zero_minimum, invert_bounds, equalize_bounds)] break_bounds: BreakBounds,
) {
    let mut config = RateBoundsConfig::default();
    break_bounds(bounds_mut(&mut config, pair));
    assert!(config.validate().is_err());
}
