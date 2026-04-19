//! SNIP-35 dynamic L2 gas pricing primitives.
//!
//! This module implements the consensus-level fee mechanism described in SNIP-35:
//! - `compute_fee_actual`: median of recent `fee_proposal` values (sliding window).
//! - `compute_fee_target`: USD-denominated target converted to FRI via STRK/USD oracle.
//! - `compute_fee_proposal`: honest proposer's recommended fee, clamped within a margin of
//!   `fee_actual`.
//!
//! See also: `fee_market` for EIP-1559-style base-fee adjustment, which receives
//! `fee_actual` as a floor.

use ethnum::U256;
use starknet_api::block::GasPrice;

#[cfg(test)]
mod test;

/// Scale factor for 18-decimal fixed-point conversion (1 STRK = 10^18 FRI).
const FRI_DECIMALS_SCALE: u128 = 10u128.pow(18);

/// Denominator for parts-per-thousand calculations in SNIP-35 fee_proposal bounds.
pub(crate) const PPT_DENOMINATOR: u128 = 1000;

/// Number of fee_proposal values used to compute fee_actual (SNIP-35).
pub(crate) const FEE_PROPOSAL_WINDOW_SIZE: usize = 10;

/// Maximum fee_proposal change per block in parts per thousand (SNIP-35: 0.2%).
pub(crate) const FEE_PROPOSAL_MARGIN_PPT: u128 = 2;

/// Target USD cost per L2 gas unit in atto-USD ($3e-9 = 3_000_000_000 atto-USD).
pub(crate) const TARGET_ATTO_USD_PER_L2_GAS: u128 = 3_000_000_000;

/// Hard minimum for the oracle-derived floor (FRI).
pub(crate) const ORACLE_L2_GAS_FLOOR_MIN_FRI: u128 = 8_000_000_000; // 8 gwei, matches MIN_ALLOWED_GAS_PRICE

/// Hard maximum for the oracle-derived floor (FRI).
pub(crate) const ORACLE_L2_GAS_FLOOR_MAX_FRI: u128 = u128::MAX;

/// Compute fee_actual from the last `window_size` fee_proposal values (SNIP-35).
/// Returns the median: for even `window_size`, the average of the two middle values rounded
/// down; for odd `window_size`, the single middle value.
/// Returns `None` if fewer than `window_size` proposals are available.
pub fn compute_fee_actual(proposals: &[GasPrice], window_size: usize) -> Option<GasPrice> {
    if proposals.len() < window_size || window_size < 2 {
        return None;
    }
    let window = &proposals[proposals.len() - window_size..];
    let mut sorted: Vec<u128> = window.iter().map(|p| p.0).collect();
    sorted.sort();
    let mid = window_size / 2;
    let median = if window_size.is_multiple_of(2) {
        // Even: average of the two middle values, rounded down.
        // Overflow-safe averaging: a + (b - a) / 2 (safe because sorted, so b >= a).
        sorted[mid - 1] + (sorted[mid] - sorted[mid - 1]) / 2
    } else {
        sorted[mid]
    };
    // Return None if median is zero (e.g., pre-SNIP-35 blocks with fee_proposal=0).
    // This triggers the l2_gas_price fallback in both proposer and validator paths.
    if median == 0 { None } else { Some(GasPrice(median)) }
}

/// Compute the fee target from STRK/USD price and a USD cost target (SNIP-35).
/// `target_atto_usd_per_l2_gas` is in atto-USD (18-decimal fixed-point).
/// `strk_usd_rate` is the STRK/USD price with 18 decimals (from oracle).
/// Result is in FRI, clamped to `[floor_min_fri, floor_max_fri]`.
pub fn compute_fee_target(
    target_atto_usd_per_l2_gas: u128,
    strk_usd_rate: u128,
    floor_min_fri: u128,
    floor_max_fri: u128,
) -> GasPrice {
    if strk_usd_rate == 0 {
        return GasPrice(floor_max_fri);
    }
    // floor_fri = target_atto_usd_per_l2_gas * 10^18 / strk_usd_rate
    let numerator = U256::from(target_atto_usd_per_l2_gas) * U256::from(FRI_DECIMALS_SCALE);
    let floor = numerator / U256::from(strk_usd_rate);
    let floor_u128 = u128::try_from(floor).unwrap_or(u128::MAX);
    GasPrice(floor_u128.clamp(floor_min_fri, floor_max_fri))
}

/// Geometric bounds for SNIP-35 fee_proposal: `(lower, upper)` where
/// `upper = fee_actual * (1 + margin)` and `lower = fee_actual / (1 + margin)`,
/// with `margin = margin_ppt / PPT_DENOMINATOR`.
///
/// The asymmetry is intentional: the bounds are geometrically symmetric (a multiplicative
/// factor in either direction), so a sequence of consecutive proposals can grow by
/// `(1 + margin)` per round or shrink by the same factor per round. Both proposer and
/// validator use this helper to ensure they agree on what's in-range.
///
/// Uses `U256` internally to keep the arithmetic mathematically correct regardless of
/// `fee_actual` and `margin_ppt`. Saturates to `u128::MAX` on the (unreachable in practice)
/// upper-end overflow.
pub(crate) fn fee_proposal_bounds(fee_actual: GasPrice, margin_ppt: u128) -> (u128, u128) {
    let denom = U256::from(PPT_DENOMINATOR);
    let scaled = denom + U256::from(margin_ppt);
    let fee_actual_u256 = U256::from(fee_actual.0);
    let upper = u128::try_from(fee_actual_u256 * scaled / denom).unwrap_or(u128::MAX);
    let lower = u128::try_from(fee_actual_u256 * denom / scaled).unwrap_or(u128::MAX);
    (lower, upper)
}

/// Compute the fee_proposal an honest proposer should publish (SNIP-35).
/// - If oracle failed (`fee_target` is `None`): freeze at `fee_actual`.
/// - Otherwise: clamp `fee_target` into the geometric bounds returned by `fee_proposal_bounds`.
pub fn compute_fee_proposal(
    fee_target: Option<GasPrice>,
    fee_actual: GasPrice,
    margin_ppt: u128,
) -> GasPrice {
    let Some(fee_target) = fee_target else {
        return fee_actual;
    };
    let (lower, upper) = fee_proposal_bounds(fee_actual, margin_ppt);
    GasPrice(fee_target.0.clamp(lower, upper))
}
