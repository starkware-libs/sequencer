use apollo_l1_gas_price_config::config::RateBoundsConfig;
use apollo_l1_gas_price_types::CurrencyPair;
use assert_matches::assert_matches;
use rstest::rstest;

use super::*;

fn micro_units_to_rate(micro_units: u64) -> ExchangeRate {
    u128::from(micro_units) * MICRO_UNIT_TO_RATE_SCALE
}

/// The production bounds for a pair, whose edges the cases below probe.
fn default_bounds(pair: CurrencyPair) -> RateBounds {
    let config = RateBoundsConfig::default();
    match pair {
        CurrencyPair::EthUsd => config.eth_usd_bounds(),
        CurrencyPair::StrkUsd => config.strk_usd_bounds(),
        CurrencyPair::EthStrk => config.eth_to_fri_bounds(),
    }
}

#[rstest]
fn a_rate_exactly_on_either_bound_is_accepted(
    #[values(CurrencyPair::EthUsd, CurrencyPair::StrkUsd, CurrencyPair::EthStrk)]
    pair: CurrencyPair,
) {
    let bounds = default_bounds(pair);
    for micro_units in [bounds.minimum_micro_units, bounds.maximum_micro_units] {
        check_rate_bounds(micro_units_to_rate(micro_units), bounds).unwrap();
    }
}

/// One unit at `RATE_DECIMALS` outside either bound is rejected, so the accepted band is exactly
/// the configured one.
#[rstest]
fn a_rate_one_unit_outside_either_bound_is_rejected(
    #[values(CurrencyPair::EthUsd, CurrencyPair::StrkUsd, CurrencyPair::EthStrk)]
    pair: CurrencyPair,
) {
    let bounds = default_bounds(pair);
    let below_the_minimum = micro_units_to_rate(bounds.minimum_micro_units) - 1;
    let above_the_maximum = micro_units_to_rate(bounds.maximum_micro_units) + 1;
    for rate in [below_the_minimum, above_the_maximum] {
        assert_matches!(
            check_rate_bounds(rate, bounds),
            Err(ExchangeRateOracleClientError::RateOutOfBoundsError { pair: error_pair, .. })
                if error_pair == pair
        );
    }
}

/// The bounds are configured in micro units and the rate arrives at `RATE_DECIMALS`, so the error
/// reports the band the rate was actually compared against.
#[test]
fn the_rejection_reports_the_band_at_rate_decimals() {
    let bounds = default_bounds(CurrencyPair::StrkUsd);
    assert_matches!(
        check_rate_bounds(0, bounds),
        Err(ExchangeRateOracleClientError::RateOutOfBoundsError { rate, min_rate, max_rate, .. })
            if rate == 0
                && min_rate == micro_units_to_rate(bounds.minimum_micro_units)
                && max_rate == micro_units_to_rate(bounds.maximum_micro_units)
    );
}
