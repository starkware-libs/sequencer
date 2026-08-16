//! The absolute sanity bounds every exchange rate must fall in, whichever source reports it.

use apollo_l1_gas_price_config::config::RateBounds;
use apollo_l1_gas_price_types::errors::ExchangeRateOracleClientError;
use apollo_l1_gas_price_types::ExchangeRate;

#[cfg(test)]
#[path = "rate_bounds_test.rs"]
mod rate_bounds_test;

/// Absolute bounds catch a feed wired to the wrong asset: consensus checks that validators agree
/// with each other, never that the agreed value is sane, and every node reads the same chain state.
/// They are wide enough to pass a manipulated but plausible answer, the STRK/USD pair alone
/// accepting anything from $0.0001 to $10. The bound relative to the previous block's implied rate,
/// which limits how fast such an answer moves the rate a node publishes, lives in
/// `apollo_consensus_orchestrator::utils`, where the previous block is in hand, so that its band is
/// centered on the block header rather than on this client's own read history and every validator
/// derives the same clamped rate.
pub(crate) fn check_rate_bounds(
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
