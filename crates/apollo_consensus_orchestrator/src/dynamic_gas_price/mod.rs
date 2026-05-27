//! Dynamic L2 gas pricing primitives.
//!
//! Spec: <https://community.starknet.io/t/snip-35-automatically-adjust-base-fee-to-strk-price/116168>
//!
//! - `compute_fee_actual`: the base fee for this block, calculated as the median of the most recent
//!   `fee_proposal` values across `window_size` blocks.
//! - `compute_fee_target`: the fee we'd *like* for this block, derived from the STRK/USD oracle
//!   quote and a configurable USD-cost target.
//! - `compute_fee_proposal`: the target, clamped within a fixed multiplicative margin of
//!   `fee_actual` (so a proposer cannot move the fee too far per round).
//!
//! See also: `fee_market` for EIP-1559-style base-fee adjustment, which receives
//! `fee_actual` as a floor.

use std::collections::BTreeMap;

use apollo_consensus::types::ProposalCommitment;
use ethnum::U256;
use serde::Serialize;
use starknet_api::block::{BlockNumber, GasPrice};
use starknet_api::block_hash::block_hash_calculator::PartialBlockHash;
use starknet_types_core::felt::Felt;
use starknet_types_core::hash::{Poseidon, StarkHash};
use tracing::warn;

#[cfg(test)]
mod test;

/// Proposer-stated fee value for a block, as it travels in the cende blob to the
/// centralized recorder. Mirrors the `FeeProposalInfo` Marshmallow dataclass on the centralized
/// (Python) side; the wire JSON shape must agree across the language boundary.
#[cfg_attr(any(feature = "testing", test), derive(serde::Deserialize, PartialEq))]
#[derive(Debug, Default, Serialize)]
pub struct FeeProposalInfo {
    /// `None` for pre-V0_14_3 blocks (no value stated by the proposer); `Some(...)` for V0_14_3+
    /// blocks. The centralized side persists this independently of `FeeMarketInfo` so
    /// existing fee market storage blobs are untouched.
    pub fee_proposal_fri: Option<GasPrice>,
}

/// Scale factor for 18-decimal fixed-point conversion (1 STRK = 10^18 FRI).
const FRI_DECIMALS_SCALE: u128 = 10u128.pow(18);

/// Denominator for parts-per-thousand calculations in fee_proposal bounds.
pub(crate) const PPT_DENOMINATOR: u128 = 1000;

/// Compute fee_actual for `height` as the median of the `fee_proposal` values
/// recorded for heights `[height - window_size, height - 1]`.
///
/// Returns `None` (after logging a warning) when any of those heights is missing from
/// `fee_proposals_window` or recorded as `None` (e.g., pre-V0_14_3 blocks). The `None`
/// case triggers the `l2_gas_price` fallback in both proposer and validator paths.
///
/// Median rule for even `window_size`: average of the two middle values rounded down;
/// for odd: the single middle value.
pub fn compute_fee_actual(
    fee_proposals_window: &BTreeMap<BlockNumber, Option<GasPrice>>,
    height: BlockNumber,
    window_size: u64,
) -> Option<GasPrice> {
    let Some(start) = height.0.checked_sub(window_size) else {
        warn!(
            "Cannot compute fee_actual for height {height}: height is below window_size \
             ({window_size})"
        );
        return None;
    };
    let window_size_usize = usize::try_from(window_size).expect("window_size fits in usize");
    let mut window = Vec::with_capacity(window_size_usize);
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
    let mid = window_size_usize / 2;
    let median = if window_size_usize.is_multiple_of(2) {
        // Even: average of the two middle values, rounded down.
        // Overflow-safe averaging: a + (b - a) / 2 (safe because sorted, so b >= a).
        GasPrice(window[mid - 1].0 + (window[mid].0 - window[mid - 1].0) / 2)
    } else {
        window[mid]
    };
    Some(median)
}

/// Compute the fee target from STRK/USD price and a USD cost target.
///
/// `target_atto_usd_per_l2_gas` is in atto-USD (atto = 10⁻¹⁸; so a value of
/// `3_000_000_000` means 3·10⁻⁹ USD = 3 nanodollars per L2 gas unit).
/// `strk_usd_rate` is the STRK/USD price with 18 decimals (from the oracle); a rate of `0`
/// returns `None` (treat as oracle failure: callers freeze at `fee_actual`).
/// Returns the target in FRI; the multiplicative margin clamp in `compute_fee_proposal`
/// keeps the published proposal bounded relative to `fee_actual`.
pub fn compute_fee_target(
    target_atto_usd_per_l2_gas: u128,
    strk_usd_rate: u128,
) -> Option<GasPrice> {
    if strk_usd_rate == 0 {
        return None;
    }
    // floor_fri = target_atto_usd_per_l2_gas * 10^18 / strk_usd_rate
    let numerator = U256::from(target_atto_usd_per_l2_gas) * U256::from(FRI_DECIMALS_SCALE);
    let floor = numerator / U256::from(strk_usd_rate);
    Some(GasPrice(u128::try_from(floor).unwrap_or(u128::MAX)))
}

/// Compute the fee_proposal an honest proposer should publish.
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

/// Geometric bounds for fee_proposal: returns `(lower, upper)` where
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

/// Bind `fee_proposal_fri` to the proposal commitment hash.
///
/// Pre-V0_14_3 blocks have `fee_proposal = None` and the commitment is just `partial.0`,
/// preserving on-chain behavior. From V0_14_3 onward, the commitment is
/// `Poseidon(partial.0, fee_proposal_fri)`, so a proposer cannot equivocate on
/// `fee_proposal_fri` without changing the commitment that consensus signs over.
///
/// This computation lives in the orchestrator (not in `starknet_api`) because the
/// fee_proposal is a consensus-time signal — `starknet_api`'s block-hash primitives
/// remain unaware of it, and `BlockHash` is unaffected.
pub(crate) fn proposal_commitment_from(
    partial: PartialBlockHash,
    fee_proposal: Option<GasPrice>,
) -> ProposalCommitment {
    let Some(fee_proposal) = fee_proposal else {
        return ProposalCommitment(partial.0);
    };
    ProposalCommitment(Poseidon::hash_array(&[partial.0, Felt::from(fee_proposal.0)]))
}
