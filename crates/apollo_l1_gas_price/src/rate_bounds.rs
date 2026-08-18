//! The absolute sanity bounds every exchange rate must fall in, whichever source reports it.

// [Temporary comment] `pub` with no production caller yet: the Chainlink feed read (A8) and the
// HTTP oracle (C1) both call this; A8 narrows it to `pub(crate)`.

use apollo_l1_gas_price_config::config::RateBounds;
use apollo_l1_gas_price_types::errors::ExchangeRateOracleClientError;
use apollo_l1_gas_price_types::ExchangeRate;

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
pub fn check_rate_bounds(
    rate: ExchangeRate,
    bounds: RateBounds,
) -> Result<(), ExchangeRateOracleClientError> {
    if rate < bounds.minimum_rate || rate > bounds.maximum_rate {
        return Err(ExchangeRateOracleClientError::RateOutOfBoundsError {
            pair: bounds.pair,
            rate,
            min_rate: bounds.minimum_rate,
            max_rate: bounds.maximum_rate,
        });
    }
    Ok(())
}
