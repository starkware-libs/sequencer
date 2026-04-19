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

/// Compute fee_actual from the last `window_size` fee_proposal values (SNIP-35).
/// Returns the average of the two middle values, rounded down.
/// Returns `None` if fewer than `window_size` proposals are available.
pub fn compute_fee_actual(proposals: &[GasPrice], window_size: usize) -> Option<GasPrice> {
    if proposals.len() < window_size || window_size < 2 {
        return None;
    }
    let window = &proposals[proposals.len() - window_size..];
    let mut sorted: Vec<u128> = window.iter().map(|p| p.0).collect();
    sorted.sort();
    // Median = average of the two middle values, rounded down.
    // Use overflow-safe averaging: a + (b - a) / 2 (safe because sorted, so b >= a).
    let mid = window_size / 2;
    let median = sorted[mid - 1] + (sorted[mid] - sorted[mid - 1]) / 2;
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

/// Compute the fee_proposal an honest proposer should publish (SNIP-35).
/// - If oracle failed (`fee_target` is `None`): freeze at `fee_actual`.
/// - Otherwise: clamp `fee_target` to within +/-`margin_ppt` parts per thousand of `fee_actual`.
pub fn compute_fee_proposal(
    fee_target: Option<GasPrice>,
    fee_actual: GasPrice,
    margin_ppt: u128,
) -> GasPrice {
    let Some(fee_target) = fee_target else {
        return fee_actual;
    };
    let upper = fee_actual.0.saturating_mul(PPT_DENOMINATOR + margin_ppt) / PPT_DENOMINATOR;
    let lower = fee_actual.0.saturating_mul(PPT_DENOMINATOR) / (PPT_DENOMINATOR + margin_ppt);
    GasPrice(fee_target.0.clamp(lower, upper))
}
