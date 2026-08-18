//! Decoding of the retdata Chainlink's feeds return.

// [Temporary comment] No production caller yet, and every item is `pub` so this module compiles
// standalone. The feed read (A8) calls these and narrows them to `pub(super)`.

use apollo_cairo_utils::{deserialize_retdata, RetdataDeserializationError, TryFromIterator};
use apollo_l1_gas_price_types::errors::ExchangeRateOracleClientError;
use apollo_l1_gas_price_types::{CurrencyPair, EXCHANGE_RATE_DECIMALS};
use starknet_types_core::felt::Felt;

#[cfg(test)]
#[path = "feed_decode_test.rs"]
mod feed_decode_test;

/// The Chainlink feeds report 8 decimals today. A range is accepted rather than the exact value so
/// that a feed upgrade does not halt pricing, bounded so the rescale to `EXCHANGE_RATE_DECIMALS`
/// can neither underflow nor produce an absurd scale factor.
pub const MIN_FEED_DECIMALS: u32 = 6;
pub const MAX_FEED_DECIMALS: u32 = EXCHANGE_RATE_DECIMALS;

/// The fields of Chainlink's `Round` that the oracle consumes.
#[derive(Debug)]
pub struct ChainlinkRoundData {
    /// The price the feed reports, at the feed's own `decimals()`.
    pub answer: u128,
    /// Unix seconds at which the aggregator last wrote this round.
    pub updated_at: u64,
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

pub fn decode_feed_decimals(
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

pub fn decode_feed_round(
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
