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

use ethnum::U256;
use serde::Serialize;
use starknet_api::block::GasPrice;

#[cfg(test)]
mod test;

/// SNIP-35 proposer-stated fee value for a block, as it travels in the cende blob to the
/// centralized recorder. Mirrors the `Snip35Info` Marshmallow dataclass on the centralized
/// (Python) side; the wire JSON shape must agree across the language boundary.
#[cfg_attr(any(feature = "testing", test), derive(serde::Deserialize, PartialEq))]
#[derive(Debug, Default, Serialize)]
pub struct Snip35Info {
    /// `None` for pre-V0_14_3 blocks (no value stated by the proposer); `Some(...)` for SNIP-35
    /// era blocks. The centralized side persists this independently of `FeeMarketInfo` so
    /// existing fee market storage blobs are untouched.
    pub fee_proposal_fri: Option<GasPrice>,
}

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
pub(crate) const TARGET_ATTO_USD_PER_L2_GAS: u128 = 3_000_000_000;

/// Hard minimum for the oracle-derived floor (FRI).
pub(crate) const ORACLE_L2_GAS_FLOOR_MIN_FRI: u128 = 8_000_000_000; // 8 gwei, matches MIN_ALLOWED_GAS_PRICE

/// Hard maximum for the oracle-derived floor (FRI).
pub(crate) const ORACLE_L2_GAS_FLOOR_MAX_FRI: u128 = u128::MAX;

/// Compute fee_actual from the last `window_size` `fee_proposal` values (SNIP-35).
///
/// Median rule: for even `window_size`, the average of the two middle values rounded
/// down; for odd `window_size`, the single middle value.
///
/// Returns `None` if `window_size == 0`, fewer than `window_size` proposals are
/// available, or the median is zero (e.g., pre-SNIP-35 blocks with `fee_proposal == 0`).
/// The `None` case triggers the `l2_gas_price` fallback in both proposer and validator
/// paths.
pub fn compute_fee_actual(proposals: &[GasPrice], window_size: usize) -> Option<GasPrice> {
    if window_size == 0 {
        return None;
    }
    let start = proposals.len().checked_sub(window_size)?;
    let window = proposals.get(start..)?;
    let mut sorted: Vec<GasPrice> = window.to_vec();
    sorted.sort();
    let mid = window_size / 2;
    let median = if window_size.is_multiple_of(2) {
        // Even: average of the two middle values, rounded down.
        // Overflow-safe averaging: a + (b - a) / 2 (safe because sorted, so b >= a).
        GasPrice(sorted[mid - 1].0 + (sorted[mid].0 - sorted[mid - 1].0) / 2)
    } else {
        sorted[mid]
    };
    if median.0 == 0 { None } else { Some(median) }
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
