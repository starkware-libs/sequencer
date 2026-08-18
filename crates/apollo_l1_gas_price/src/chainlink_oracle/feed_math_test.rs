use assert_matches::assert_matches;
use rstest::rstest;

use super::*;
use crate::chainlink_oracle::test_utils::{
    decimals_retdata,
    round_retdata,
    ETH_TO_FRI_RATE,
    ETH_USD_ANSWER,
    FEED_DECIMALS,
    STRK_TO_USD_RATE,
    STRK_USD_ANSWER,
};

const UPDATED_AT: u64 = 1_700_000_000;

/// The two fields the oracle reads sit at positions two and five of the flat five-felt `Round`, so
/// this pins the layout the decoder assumes.
#[test]
fn round_data_decodes_the_answer_and_the_update_time() {
    let round: ChainlinkRoundData =
        decode_retdata(round_retdata(STRK_USD_ANSWER, UPDATED_AT)).unwrap();
    assert_eq!(round.answer, STRK_USD_ANSWER);
    assert_eq!(round.updated_at, UPDATED_AT);
}

#[rstest]
#[case::too_few_felts(round_retdata(STRK_USD_ANSWER, UPDATED_AT).into_iter().take(4).collect())]
#[case::too_many_felts([round_retdata(STRK_USD_ANSWER, UPDATED_AT), vec![Felt::ONE]].concat())]
#[case::answer_exceeding_u128(vec![Felt::ONE, Felt::MAX, Felt::ONE, Felt::ONE, Felt::ONE])]
#[case::updated_at_exceeding_u64(
    vec![Felt::ONE, Felt::from(STRK_USD_ANSWER), Felt::ONE, Felt::ONE, Felt::MAX]
)]
fn malformed_retdata_rejected(#[case] malformed_round_retdata: Vec<Felt>) {
    assert_matches!(
        decode_retdata::<ChainlinkRoundData>(malformed_round_retdata),
        Err(ExchangeRateOracleClientError::ParseError(_))
    );
}

/// The accepted range is what bounds the rescale; a feed reporting outside it is rejected rather
/// than mis-scaled.
#[rstest]
#[case::at_the_minimum(MIN_FEED_DECIMALS, true)]
#[case::at_the_maximum(MAX_FEED_DECIMALS, true)]
#[case::below_the_minimum(MIN_FEED_DECIMALS - 1, false)]
#[case::above_the_maximum(MAX_FEED_DECIMALS + 1, false)]
fn feed_decimals_range_is_enforced(#[case] feed_decimals: u32, #[case] is_accepted: bool) {
    let result = decode_feed_decimals(decimals_retdata(feed_decimals), CurrencyPair::StrkUsd);
    if is_accepted {
        assert_eq!(result.unwrap(), feed_decimals);
    } else {
        assert_matches!(
            result,
            Err(ExchangeRateOracleClientError::InvalidRateError(message))
                if message.contains("decimals")
        );
    }
}

/// A decimals value too large for a `u32` must be reported as a parse failure rather than
/// truncated into a plausible scale.
#[test]
fn feed_decimals_exceeding_u32_rejected() {
    assert_matches!(
        decode_feed_decimals(vec![Felt::MAX], CurrencyPair::StrkUsd),
        Err(ExchangeRateOracleClientError::ParseError(message)) if message.contains("u32")
    );
}

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

/// Each answer is rescaled to `RATE_DECIMALS` before the division, so the derived rate comes out
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

/// The two legs do not divide evenly here, so this is the case that exercises recombining the
/// scaled quotient with the scaled remainder, the one step of the derivation whose result cannot be
/// read off the inputs.
#[test]
fn eth_to_fri_recombines_a_non_zero_remainder() {
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

#[test]
fn eth_to_fri_errors_instead_of_overflowing() {
    assert_matches!(
        derive_eth_to_fri_rate(u128::MAX, 1),
        Err(ExchangeRateOracleClientError::ArithmeticError(_))
    );
}

#[rstest]
#[case::ascii("a plain revert reason".to_string())]
#[case::multibyte("שלום".repeat(10))]
fn short_contract_call_error_is_relayed_verbatim(#[case] error_text: String) {
    assert!(error_text.len() <= MAX_CONTRACT_CALL_ERROR_BYTES);
    assert_eq!(truncate_contract_call_error(error_text.clone()), error_text);
}

/// The cap counts bytes, so a multi-byte reason must be cut at a character boundary at or just
/// below it, never mid-character.
#[rstest]
#[case::single_byte_characters("a")]
#[case::four_byte_characters("😀")]
fn long_contract_call_error_is_truncated_on_a_character_boundary(#[case] repeated_text: &str) {
    const NUM_REPETITIONS: usize = 1000;
    let error_text = repeated_text.repeat(NUM_REPETITIONS);
    let truncated = truncate_contract_call_error(error_text.clone());

    let head = truncated
        .strip_suffix(TRUNCATION_MARKER)
        .expect("Truncated text must carry the truncation marker");
    assert!(error_text.starts_with(head), "the kept head must be a prefix of the original");
    assert!(head.len() <= MAX_CONTRACT_CALL_ERROR_BYTES);
    // Nothing is dropped beyond what the boundary requires.
    assert!(head.len() > MAX_CONTRACT_CALL_ERROR_BYTES - repeated_text.len());
}
