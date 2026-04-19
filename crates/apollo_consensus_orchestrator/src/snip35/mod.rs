//! SNIP-35 dynamic L2 gas pricing primitives.
//!
//! Spec: <https://community.starknet.io/t/snip-35-automatically-adjust-base-fee-to-strk-price/116168>
//!
//! - `compute_fee_actual`: the base fee for this block, calculated as the median of the most recent
//!   `fee_proposal` values across `window_size` blocks.
//! - `compute_fee_target`: the fee we'd *like* for this block, derived from the STRK/USD oracle
//!   quote and a fixed USD-cost target.
//! - `compute_fee_proposal`: the target, clamped within a fixed multiplicative margin of
//!   `fee_actual` (so a proposer cannot move the fee too far per round).
//!
//! See also: `fee_market` for EIP-1559-style base-fee adjustment, which receives
//! `fee_actual` as a floor.

use std::collections::BTreeMap;

use ethnum::U256;
use starknet_api::block::{BlockNumber, GasPrice};
use tracing::warn;

#[cfg(test)]
mod test;

/// Scale factor for 18-decimal fixed-point conversion (1 STRK = 10^18 FRI).
const FRI_DECIMALS_SCALE: u128 = 10u128.pow(18);

/// Denominator for parts-per-thousand calculations in SNIP-35 fee_proposal bounds.
pub(crate) const PPT_DENOMINATOR: u128 = 1000;

/// Number of fee_proposal values used to compute fee_actual (SNIP-35).
// TODO(AndrewL): consider moving this to versioned constants.
pub(crate) const FEE_PROPOSAL_WINDOW_SIZE: usize = 10;

/// Maximum fee_proposal change per block in parts per thousand (SNIP-35: 0.2%).
pub(crate) const FEE_PROPOSAL_MARGIN_PPT: u128 = 2;

/// Target USD cost per L2 gas unit in atto-USD ($3e-9 = 3_000_000_000 atto-USD).
// TODO(AndrewL): consider moving this to versioned constants.
pub(crate) const TARGET_ATTO_USD_PER_L2_GAS: u128 = 3_000_000_000;

/// Hard minimum for the oracle-derived floor (FRI).
// TODO(AndrewL): consider moving this to versioned constants as a separate field (the
// existing `min_gas_price = 8 gwei` matches the current value, but the SNIP-35 oracle floor
// is a distinct knob).
pub(crate) const ORACLE_L2_GAS_FLOOR_MIN_FRI: u128 = 8_000_000_000; // 8 gwei, matches MIN_ALLOWED_GAS_PRICE

/// Hard maximum for the oracle-derived floor (FRI).
pub(crate) const ORACLE_L2_GAS_FLOOR_MAX_FRI: u128 = u128::MAX;

/// Compute fee_actual for `height` as the median of the `fee_proposal` values
/// recorded for heights `[height - FEE_PROPOSAL_WINDOW_SIZE, height - 1]` (SNIP-35).
///
/// Returns `None` (after logging a warning) when any of those heights is missing from
/// `fee_proposals_window` or recorded as `None` (e.g., pre-SNIP-35 blocks). The `None`
/// case triggers the `l2_gas_price` fallback in both proposer and validator paths.
///
/// Median rule for even `FEE_PROPOSAL_WINDOW_SIZE`: average of the two middle values
/// rounded down; for odd: the single middle value.
pub fn compute_fee_actual(
    fee_proposals_window: &BTreeMap<BlockNumber, Option<GasPrice>>,
    height: BlockNumber,
) -> Option<GasPrice> {
    let window_size =
        u64::try_from(FEE_PROPOSAL_WINDOW_SIZE).expect("FEE_PROPOSAL_WINDOW_SIZE fits in u64");
    let Some(start) = height.0.checked_sub(window_size) else {
        warn!(
            "Cannot compute fee_actual for height {height}: height is below \
             FEE_PROPOSAL_WINDOW_SIZE ({FEE_PROPOSAL_WINDOW_SIZE})"
        );
        return None;
    };
    let mut window = Vec::with_capacity(FEE_PROPOSAL_WINDOW_SIZE);
    for source_height in (start..height.0).map(BlockNumber) {
        match fee_proposals_window.get(&source_height) {
            Some(Some(price)) => window.push(*price),
            Some(None) | None => {
                warn!(
                    "Cannot compute fee_actual for height {height}: fee_proposals_window has no \
                     recorded fee_proposal for height {source_height}"
                );
                return None;
            }
        }
    }
    window.sort();
    let mid = FEE_PROPOSAL_WINDOW_SIZE / 2;
    let median = if FEE_PROPOSAL_WINDOW_SIZE.is_multiple_of(2) {
        // Even: average of the two middle values, rounded down.
        // Overflow-safe averaging: a + (b - a) / 2 (safe because sorted, so b >= a).
        GasPrice(window[mid - 1].0 + (window[mid].0 - window[mid - 1].0) / 2)
    } else {
        window[mid]
    };
    Some(median)
}

/// Compute the fee target from STRK/USD price and a USD cost target (SNIP-35).
///
/// `target_atto_usd_per_l2_gas` is in atto-USD (atto = 10⁻¹⁸; so a value of
/// `3_000_000_000` means 3·10⁻⁹ USD = 3 nanodollars per L2 gas unit).
/// `strk_usd_rate` is the STRK/USD price with 18 decimals (from the oracle).
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

/// Geometric bounds for SNIP-35 fee_proposal: returns `(lower, upper)` where
/// - `upper = fee_actual * (1 + margin)` (multiplicative widening), and
/// - `lower = fee_actual / (1 + margin)` (the reciprocal — multiplicative narrowing),
///
/// with `margin = margin_ppt / PPT_DENOMINATOR`.
///
/// The asymmetry is intentional: bounds are geometrically symmetric (the same
/// multiplicative factor in either direction), so a sequence of consecutive proposals
/// can grow by `(1 + margin)` per round or shrink by the same factor per round. Both
/// proposer and validator use this helper to ensure they agree on what's in-range.
///
/// Uses `U256` internally to keep the arithmetic mathematically correct regardless of
/// `fee_actual` and `margin_ppt`. On the practically-unreachable overflow, the upper
/// bound saturates to `u128::MAX` and the lower bound saturates to `0`.
pub(crate) fn fee_proposal_bounds(fee_actual: GasPrice, margin_ppt: u128) -> (u128, u128) {
    let denom = U256::from(PPT_DENOMINATOR);
    let scaled = denom + U256::from(margin_ppt);
    let fee_actual_u256 = U256::from(fee_actual.0);
    let upper = u128::try_from(fee_actual_u256 * scaled / denom).unwrap_or(u128::MAX);
    let lower = u128::try_from(fee_actual_u256 * denom / scaled).unwrap_or(0);
    (lower, upper)
}
