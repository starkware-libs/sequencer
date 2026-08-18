//! Decoding of Chainlink feed retdata, and the fixed-point arithmetic over the decoded answers.

use apollo_cairo_utils::{deserialize_retdata, RetdataDeserializationError, TryFromIterator};
use apollo_l1_gas_price_types::errors::ExchangeRateOracleClientError;
use apollo_l1_gas_price_types::{
    CurrencyPair,
    ExchangeRate,
    EXCHANGE_RATE_DECIMALS,
    EXCHANGE_RATE_SCALE,
};
use ethnum::U256;
use starknet_types_core::felt::Felt;

#[cfg(test)]
#[path = "feed_math_test.rs"]
mod feed_math_test;

/// The Chainlink feeds report 8 decimals today. A range is accepted rather than the exact value so
/// that a feed upgrade does not halt pricing, bounded so the rescale to `EXCHANGE_RATE_DECIMALS`
/// can neither underflow nor produce an absurd scale factor.
const MIN_FEED_DECIMALS: u32 = 6;
const MAX_FEED_DECIMALS: u32 = EXCHANGE_RATE_DECIMALS;

/// Cap on the batcher error text the Chainlink oracle relays. A reverting view call's panic data
/// reaches the logs, the failure cache, and (when the provider runs remotely) the RPC boundary, so
/// the cap is byte-based to bound what all three consume.
pub(super) const MAX_CONTRACT_CALL_ERROR_BYTES: usize = 256;
pub(super) const TRUNCATION_MARKER: &str = "...[truncated]";

/// A rate at `EXCHANGE_RATE_DECIMALS`, or the guard trip that rejected it.
pub(super) type RateResult = Result<ExchangeRate, ExchangeRateOracleClientError>;

pub(super) fn truncate_contract_call_error(error_text: String) -> String {
    if error_text.len() <= MAX_CONTRACT_CALL_ERROR_BYTES {
        return error_text;
    }
    // Cut on a character boundary so the relayed text stays valid UTF-8. The nearest boundary at
    // or below the cap is at most three bytes down.
    let head_end = (0..=MAX_CONTRACT_CALL_ERROR_BYTES)
        .rev()
        .find(|byte_index| error_text.is_char_boundary(*byte_index))
        .expect("Byte index 0 is always a character boundary");
    format!("{}{TRUNCATION_MARKER}", &error_text[..head_end])
}

pub(super) fn decode_feed_decimals(
    decimals_retdata: Vec<Felt>,
    pair: CurrencyPair,
) -> Result<u32, ExchangeRateOracleClientError> {
    let pair_name = pair.pair_name();
    let raw_decimals: Felt = decode_retdata(decimals_retdata)?;
    let feed_decimals = u32::try_from(raw_decimals).map_err(|_| {
        ExchangeRateOracleClientError::ParseError(format!(
            "{pair_name} decimals {raw_decimals} does not fit in u32"
        ))
    })?;
    if !(MIN_FEED_DECIMALS..=MAX_FEED_DECIMALS).contains(&feed_decimals) {
        return Err(ExchangeRateOracleClientError::InvalidRateError(format!(
            "{pair_name} reports {feed_decimals} decimals, outside the accepted range \
             [{MIN_FEED_DECIMALS}, {MAX_FEED_DECIMALS}]"
        )));
    }
    Ok(feed_decimals)
}

pub(super) fn decode_feed_round(
    round_retdata: Vec<Felt>,
) -> Result<ChainlinkRoundData, ExchangeRateOracleClientError> {
    decode_retdata(round_retdata)
}

fn decode_retdata<T>(retdata: Vec<Felt>) -> Result<T, ExchangeRateOracleClientError>
where
    T: TryFromIterator<Felt, Error = RetdataDeserializationError>,
{
    deserialize_retdata(retdata)
        .map_err(|error| ExchangeRateOracleClientError::ParseError(error.to_string()))
}

pub(super) fn rescale_to_rate_decimals(answer: u128, feed_decimals: u32) -> RateResult {
    EXCHANGE_RATE_DECIMALS
        .checked_sub(feed_decimals)
        .and_then(|exponent| 10u128.checked_pow(exponent))
        .and_then(|scale| answer.checked_mul(scale))
        .ok_or_else(|| {
            ExchangeRateOracleClientError::ArithmeticError(format!(
                "rescaling answer {answer} from {feed_decimals} to {EXCHANGE_RATE_DECIMALS} \
                 decimals overflowed"
            ))
        })
}

/// STRK per ETH, at `EXCHANGE_RATE_DECIMALS`, from two USD prices that already carry
/// `EXCHANGE_RATE_DECIMALS`.
pub(super) fn derive_eth_to_fri_rate(
    eth_to_usd_rate: ExchangeRate,
    strk_to_usd_rate: ExchangeRate,
) -> RateResult {
    if strk_to_usd_rate == 0 {
        return Err(ExchangeRateOracleClientError::ArithmeticError(
            "deriving ETH/STRK from a zero strk_to_usd_rate".to_string(),
        ));
    }
    // The division cancels the two operands' scales, so the numerator is scaled back up by
    // `EXCHANGE_RATE_SCALE` first. U256 because that product overflows u128 for any realistic ETH
    // price; the quotient is back within u128 whenever the two legs are within their bounds.
    let scaled_rate = (U256::from(eth_to_usd_rate) * U256::from(EXCHANGE_RATE_SCALE))
        / U256::from(strk_to_usd_rate);
    ExchangeRate::try_from(scaled_rate).map_err(|_| {
        ExchangeRateOracleClientError::ArithmeticError(format!(
            "deriving ETH/STRK from eth_to_usd_rate={eth_to_usd_rate} and \
             strk_to_usd_rate={strk_to_usd_rate} overflowed"
        ))
    })
}

/// The fields of Chainlink's `Round` that the oracle consumes.
#[derive(Debug)]
pub(super) struct ChainlinkRoundData {
    /// The price the feed reports, at the feed's own `decimals()`.
    pub(super) answer: u128,
    /// Unix seconds at which the aggregator last wrote this round.
    pub(super) updated_at: u64,
}

impl TryFromIterator<Felt> for ChainlinkRoundData {
    type Error = RetdataDeserializationError;

    // `latest_round_data` returns `Round { round_id: felt252, answer: u128, block_num: u64,
    // started_at: u64, updated_at: u64 }`, serialized flat as exactly five felts in that order.
    fn try_from_iter<T: Iterator<Item = Felt>>(iter: &mut T) -> Result<Self, Self::Error> {
        // `round_id` is phase-encoded as `(phase_id << 128) | aggregator_round_id`, so it exceeds
        // every primitive integer type and is consumed without being decoded.
        let _round_id = Felt::try_from_iter(iter)?;
        // `answer` is unsigned on the Starknet feeds, so there is no sign extension to undo.
        let raw_answer = Felt::try_from_iter(iter)?;
        let answer = u128::try_from(raw_answer)
            .map_err(|_| RetdataDeserializationError::U128ConversionError { felt: raw_answer })?;
        let _block_number = Felt::try_from_iter(iter)?;
        // `started_at` is consumed without being decoded: an aggregator that can lie about
        // `updated_at` can lie about `started_at` too, so `started_at <= updated_at` adds no
        // guarantee beyond the freshness window enforced on `updated_at`.
        let _started_at = Felt::try_from_iter(iter)?;
        let raw_updated_at = Felt::try_from_iter(iter)?;
        let updated_at = u64::try_from(raw_updated_at).map_err(|_| {
            RetdataDeserializationError::U64ConversionError { felt: raw_updated_at }
        })?;
        Ok(Self { answer, updated_at })
    }
}
