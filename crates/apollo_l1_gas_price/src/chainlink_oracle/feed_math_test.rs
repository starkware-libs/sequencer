use assert_matches::assert_matches;
use rstest::rstest;

use super::*;
use crate::chainlink_oracle::feed_decode::{MAX_FEED_DECIMALS, MIN_FEED_DECIMALS};
use crate::chainlink_oracle::test_utils::{
    ETH_TO_FRI_RATE,
    ETH_USD_ANSWER,
    FEED_DECIMALS,
    STRK_TO_USD_RATE,
    STRK_USD_ANSWER,
};

/// $0.03 per STRK reaches the same rate from every scale a feed may report it at.
#[rstest]
#[case::at_the_minimum(MIN_FEED_DECIMALS)]
#[case::todays_feeds(FEED_DECIMALS)]
#[case::at_the_maximum(MAX_FEED_DECIMALS)]
fn rescaling_lands_every_feed_scale_on_rate_decimals(#[case] feed_decimals: u32) {
    let answer = 3 * 10u128.pow(feed_decimals) / 100;
    assert_eq!(rescale_to_rate_decimals(answer, feed_decimals).unwrap(), STRK_TO_USD_RATE);
}

#[test]
fn extreme_answer_errors_instead_of_overflowing() {
    assert_matches!(
        rescale_to_rate_decimals(u128::MAX, FEED_DECIMALS),
        Err(ExchangeRateOracleClientError::ArithmeticError(_))
    );
}

#[test]
fn eth_to_fri_divides_the_two_usd_legs() {
    // $3000 per ETH over $0.03 per STRK is 100,000 STRK per ETH.
    let eth_to_usd_rate = rescale_to_rate_decimals(ETH_USD_ANSWER, FEED_DECIMALS).unwrap();
    let strk_to_usd_rate = rescale_to_rate_decimals(STRK_USD_ANSWER, FEED_DECIMALS).unwrap();
    assert_eq!(derive_eth_to_fri_rate(eth_to_usd_rate, strk_to_usd_rate).unwrap(), ETH_TO_FRI_RATE);
}

/// Each answer is rescaled to `EXCHANGE_RATE_DECIMALS` before the division, so the derived rate
/// the same whatever scales the two feeds report at. The widest pairs also cover the rescale of the
/// largest answer accepted without overflowing.
#[rstest]
#[case::equal_decimals(8, 8)]
#[case::wider_strk_feed(8, 12)]
#[case::widest_strk_feed(6, 18)]
#[case::widest_eth_feed(18, 6)]
fn eth_to_fri_is_independent_of_the_feeds_decimals(
    #[case] eth_usd_decimals: u32,
    #[case] strk_usd_decimals: u32,
) {
    // $3000 per ETH and $0.03 per STRK, expressed at each feed's own scale.
    let eth_to_usd_rate =
        rescale_to_rate_decimals(3000 * 10u128.pow(eth_usd_decimals), eth_usd_decimals).unwrap();
    let strk_to_usd_rate =
        rescale_to_rate_decimals(3 * 10u128.pow(strk_usd_decimals) / 100, strk_usd_decimals)
            .unwrap();
    assert_eq!(derive_eth_to_fri_rate(eth_to_usd_rate, strk_to_usd_rate).unwrap(), ETH_TO_FRI_RATE);
}

/// The two legs do not divide evenly here, so the derivation truncates rather than landing on a
/// round result.
#[test]
fn eth_to_fri_truncates_a_non_zero_remainder() {
    /// $0.07 per STRK at `FEED_DECIMALS`.
    const STRK_USD_ANSWER_SEVEN_CENTS: u128 = 7_000_000;
    /// floor(3000 / 0.07 * 10^18), that is 42857.142857... STRK per ETH.
    const EXPECTED_RATE: u128 = 42_857_142_857_142_857_142_857;
    let eth_to_usd_rate = rescale_to_rate_decimals(ETH_USD_ANSWER, FEED_DECIMALS).unwrap();
    let strk_to_usd_rate =
        rescale_to_rate_decimals(STRK_USD_ANSWER_SEVEN_CENTS, FEED_DECIMALS).unwrap();
    assert_eq!(derive_eth_to_fri_rate(eth_to_usd_rate, strk_to_usd_rate).unwrap(), EXPECTED_RATE);
}

#[test]
fn eth_to_fri_errors_on_a_zero_strk_leg() {
    assert_matches!(
        derive_eth_to_fri_rate(STRK_TO_USD_RATE, 0),
        Err(ExchangeRateOracleClientError::ArithmeticError(_))
    );
}

/// The derivation carries a STRK leg far above the configured $10 ceiling, with a remainder large
/// enough that scaling it alone would not fit a u128. The function imposes no ceiling of its own;
/// the legs are bounded by `check_rate_bounds` before they reach it.
#[test]
fn eth_to_fri_derives_a_high_strk_leg_with_a_large_remainder() {
    /// $1,000 per STRK.
    const STRK_TO_USD_RATE_HIGH: u128 = 1_000 * EXCHANGE_RATE_SCALE;
    /// $50,800 per ETH, leaving a remainder of 800 STRK-units against the leg above.
    const ETH_TO_USD_RATE_HIGH: u128 = 50_800 * EXCHANGE_RATE_SCALE;
    /// 50.8 STRK per ETH.
    const EXPECTED_RATE: u128 = 50_800_000_000_000_000_000;
    assert_eq!(
        derive_eth_to_fri_rate(ETH_TO_USD_RATE_HIGH, STRK_TO_USD_RATE_HIGH).unwrap(),
        EXPECTED_RATE
    );
}

#[test]
fn eth_to_fri_errors_instead_of_overflowing() {
    assert_matches!(
        derive_eth_to_fri_rate(u128::MAX, 1),
        Err(ExchangeRateOracleClientError::ArithmeticError(_))
    );
}
