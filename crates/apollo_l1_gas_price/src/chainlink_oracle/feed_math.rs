//! Fixed-point arithmetic over the rates Chainlink's feeds report.

// [Temporary comment] No production caller yet, and every item is `pub` so this module compiles
// standalone. The feed read (A8) and the client (A9) call these and narrow them to `pub(super)`.

use apollo_l1_gas_price_types::errors::ExchangeRateOracleClientError;
use apollo_l1_gas_price_types::{ExchangeRate, EXCHANGE_RATE_DECIMALS, EXCHANGE_RATE_SCALE};
use ethnum::U256;

#[cfg(test)]
#[path = "feed_math_test.rs"]
mod feed_math_test;

/// A rate at `EXCHANGE_RATE_DECIMALS`, or the guard trip that rejected it.
pub type RateResult = Result<ExchangeRate, ExchangeRateOracleClientError>;

pub fn rescale_to_rate_decimals(raw_rate: u128, feed_decimals: u32) -> RateResult {
    EXCHANGE_RATE_DECIMALS
        .checked_sub(feed_decimals)
        .and_then(|exponent| 10u128.checked_pow(exponent))
        .and_then(|scale| raw_rate.checked_mul(scale))
        .ok_or_else(|| {
            ExchangeRateOracleClientError::ArithmeticError(format!(
                "rescaling raw rate {raw_rate} from {feed_decimals} to {EXCHANGE_RATE_DECIMALS} \
                 decimals overflowed"
            ))
        })
}

/// STRK per ETH, at `EXCHANGE_RATE_DECIMALS`, from two USD prices that already carry
/// `EXCHANGE_RATE_DECIMALS`.
pub fn derive_eth_to_fri_rate(
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
