//! The absolute sanity bounds every exchange rate must fall in, whichever source reports it.

use apollo_l1_gas_price_config::config::RateBounds;
use apollo_l1_gas_price_types::errors::ExchangeRateOracleClientError;
use apollo_l1_gas_price_types::ExchangeRate;

use crate::chainlink_oracle::feed_math::MICRO_UNIT_TO_RATE_SCALE;
use crate::metrics::CHAINLINK_ORACLE_RATE_OUT_OF_BOUNDS_COUNT;

#[cfg(test)]
#[path = "rate_bounds_test.rs"]
mod rate_bounds_test;

// TODO(Asaf): bound the rate's change against the previous block's implied rate. The absolute
// bounds below are wide enough to pass a manipulated but plausible answer, the STRK/USD pair alone
// accepting anything from $0.0001 to $10, which only a bound relative to the last accepted rate
// catches. It must be anchored to the block header rather than to node-local history, so that every
// validator accepts and rejects the same values.
/// Absolute bounds are the only defense against a feed wired to the wrong asset or a
/// plausible-but-poisoned answer: consensus checks that validators agree with each other, never
/// that the agreed value is sane, and every node reads the same chain state.
pub(crate) fn check_rate_bounds(
    rate: ExchangeRate,
    bounds: RateBounds,
) -> Result<(), ExchangeRateOracleClientError> {
    let pair = bounds.pair;
    let min_rate = u128::from(bounds.minimum_micro_units).saturating_mul(MICRO_UNIT_TO_RATE_SCALE);
    let max_rate = u128::from(bounds.maximum_micro_units).saturating_mul(MICRO_UNIT_TO_RATE_SCALE);
    if rate < min_rate || rate > max_rate {
        CHAINLINK_ORACLE_RATE_OUT_OF_BOUNDS_COUNT.increment(1, &pair.labels());
        return Err(ExchangeRateOracleClientError::RateOutOfBoundsError {
            pair_name: pair.pair_name().to_string(),
            rate,
            min_rate,
            max_rate,
        });
    }
    Ok(())
}
