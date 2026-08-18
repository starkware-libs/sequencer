use assert_matches::assert_matches;
use rstest::rstest;
use starknet_types_core::felt::Felt;

use super::*;

/// $0.03 per STRK at the scale the Chainlink feeds report at today.
const STRK_USD_ANSWER: u128 = 3_000_000;
const UPDATED_AT: u64 = 1_700_000_000;

pub(super) fn decimals_retdata(feed_decimals: u32) -> Vec<Felt> {
    vec![Felt::from(feed_decimals)]
}

pub(super) fn round_retdata(answer: u128, updated_at: u64) -> Vec<Felt> {
    // A realistic phase-encoded `round_id`: `(phase_id << 128) | aggregator_round_id`, which
    // exceeds u64.
    const PHASE_ENCODED_ROUND_ID: &str = "0x100000000000000000000000000000042";
    const BLOCK_NUMBER: u64 = 987_654;
    const STARTED_AT: u64 = 1_699_999_000;
    vec![
        Felt::from_hex_unchecked(PHASE_ENCODED_ROUND_ID),
        Felt::from(answer),
        Felt::from(BLOCK_NUMBER),
        Felt::from(STARTED_AT),
        Felt::from(updated_at),
    ]
}

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
