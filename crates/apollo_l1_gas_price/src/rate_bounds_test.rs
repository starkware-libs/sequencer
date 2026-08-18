use apollo_l1_gas_price_config::config::AllRateBoundsConfig;
use apollo_l1_gas_price_types::CurrencyPair;
use assert_matches::assert_matches;
use rstest::rstest;

use super::*;

/// The production bounds for a pair, whose edges the cases below probe.
fn default_bounds(pair: CurrencyPair) -> RateBounds {
    let config = AllRateBoundsConfig::default();
    match pair {
        CurrencyPair::EthUsd => config.eth_usd_bounds(),
        CurrencyPair::StrkUsd => config.strk_usd_bounds(),
        CurrencyPair::EthStrk => config.eth_strk_bounds(),
    }
}

#[rstest]
fn a_rate_exactly_on_either_bound_is_accepted(
    #[values(CurrencyPair::EthUsd, CurrencyPair::StrkUsd, CurrencyPair::EthStrk)]
    pair: CurrencyPair,
) {
    let bounds = default_bounds(pair);
    for rate in [bounds.minimum_rate, bounds.maximum_rate] {
        check_rate_bounds(rate, bounds).unwrap();
    }
}

/// One unit at `EXCHANGE_RATE_DECIMALS` outside either bound is rejected, so the accepted band is
/// exactly the configured one.
#[rstest]
fn a_rate_one_unit_outside_either_bound_is_rejected(
    #[values(CurrencyPair::EthUsd, CurrencyPair::StrkUsd, CurrencyPair::EthStrk)]
    pair: CurrencyPair,
) {
    let bounds = default_bounds(pair);
    for rate in [bounds.minimum_rate - 1, bounds.maximum_rate + 1] {
        assert_matches!(
            check_rate_bounds(rate, bounds),
            Err(ExchangeRateOracleClientError::RateOutOfBoundsError { pair: error_pair, .. })
                if error_pair == pair
        );
    }
}

/// The rejection carries the band the rate was compared against, so an operator reading the log
/// sees the bounds in the same scale as the rate.
#[test]
fn the_rejection_reports_the_band_it_compared_against() {
    let bounds = default_bounds(CurrencyPair::StrkUsd);
    assert_matches!(
        check_rate_bounds(0, bounds),
        Err(ExchangeRateOracleClientError::RateOutOfBoundsError { rate, min_rate, max_rate, .. })
            if rate == 0 && min_rate == bounds.minimum_rate && max_rate == bounds.maximum_rate
    );
}
