use std::cmp::max;

use apollo_consensus_orchestrator_config::config::PricePerHeight;
use ethnum::U256;
use orchestrator_versioned_constants::VersionedConstants;
use serde::Serialize;
use starknet_api::block::{BlockNumber, GasPrice};
use starknet_api::execution_resources::GasAmount;
use starknet_api::versioned_constants_logic::VersionedConstantsTrait;
use tracing::info;

use crate::orchestrator_versioned_constants;

#[cfg(test)]
mod test;

// Denominator for the maximum gas price increase per block when price is below minimum.
// This controls how quickly the gas price can rise towards the minimum.
//
// With a denominator of 333: Each block can increase by at most 0.3% of the current price, to
// double the price takes approximately 230 blocks (at 2.6 seconds per block), this means doubling
// in approximately 10 minutes.
const MIN_GAS_PRICE_INCREASE_DENOMINATOR: u128 = 333;

/// Scale factor for 18-decimal fixed-point conversion (1 STRK = 10^18 FRI).
const FRI_DECIMALS_SCALE: u128 = 10u128.pow(18);

/// Denominator for parts-per-thousand calculations in SNIP-35 fee_proposal bounds.
pub(crate) const PPT_DENOMINATOR: u128 = 1000;

/// Number of fee_proposal values used to compute fee_actual (SNIP-35).
pub const FEE_PROPOSAL_WINDOW_SIZE: usize = 10;

/// Maximum fee_proposal change per block in parts per thousand (SNIP-35: 0.2%).
pub const FEE_PROPOSAL_MARGIN_PPT: u128 = 2;

/// Target USD cost per L2 gas unit in atto-USD ($3e-9 = 3_000_000_000 atto-USD).
pub const TARGET_ATTO_USD_PER_L2_GAS: u128 = 3_000_000_000;

/// Hard minimum for the oracle-derived floor (FRI).
pub const ORACLE_L2_GAS_FLOOR_MIN_FRI: u128 = 8_000_000_000; // 8 gwei, matches MIN_ALLOWED_GAS_PRICE

/// Hard maximum for the oracle-derived floor (FRI).
pub const ORACLE_L2_GAS_FLOOR_MAX_FRI: u128 = u128::MAX;

/// Fee market information for the next block.
#[derive(Debug, Default, Serialize)]
pub struct FeeMarketInfo {
    /// Total gas consumed in the current block.
    pub l2_gas_consumed: GasAmount,
    /// Gas price for the next block.
    pub next_l2_gas_price: GasPrice,
}

/// Get the minimum gas price for a given block height from the min_l2_gas_price_per_height
/// configuration. If not exist for the given height, use versioned constants min_gas_price as
/// fallback.
///
/// # Parameters
/// - `height`: The block height to look up.
/// - `min_l2_gas_price_per_height`: List of height-price pairs from configuration, assumed to be
///   sorted by height in ascending order.
pub fn get_min_gas_price_for_height(
    height: BlockNumber,
    min_l2_gas_price_per_height: &[PricePerHeight],
) -> GasPrice {
    let fallback_min_gas_price =
        orchestrator_versioned_constants::VersionedConstants::latest_constants().min_gas_price;
    min_l2_gas_price_per_height
        .iter()
        .rev()
        .find(|e| e.height <= height.0)
        .map(|e| GasPrice(e.price))
        .unwrap_or(fallback_min_gas_price)
}

/// Compute the next L2 gas price (for the fin or for updating state). Respects override when set.
pub fn calculate_next_l2_gas_price_for_fin(
    current_l2_gas_price: GasPrice,
    height: BlockNumber,
    l2_gas_used: GasAmount,
    override_l2_gas_price_fri: Option<u128>,
    min_l2_gas_price_per_height: &[PricePerHeight],
    fee_actual: Option<GasPrice>,
) -> GasPrice {
    if let Some(override_value) = override_l2_gas_price_fri {
        info!(
            "L2 gas price ({}) is not updated, remains on override value of {override_value} fri",
            current_l2_gas_price.0
        );
        return GasPrice(override_value);
    }
    let gas_target = VersionedConstants::latest_constants().gas_target;
    let config_min = get_min_gas_price_for_height(height, min_l2_gas_price_per_height);
    let effective_min = match fee_actual {
        Some(fa) => GasPrice(max(config_min.0, fa.0)),
        None => config_min,
    };
    calculate_next_base_gas_price(current_l2_gas_price, l2_gas_used, gas_target, effective_min)
}

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

/// Calculate the base gas price for the next block according to EIP-1559.
///
/// # Parameters
/// - `price`: The base gas price per unit (in fri) of the current block.
/// - `gas_used`: The total gas used in the current block.
/// - `gas_target`: The target gas usage per block.
/// - `min_gas_price`: The minimum gas price to enforce.
pub fn calculate_next_base_gas_price(
    price: GasPrice,
    gas_used: GasAmount,
    gas_target: GasAmount,
    min_gas_price: GasPrice,
) -> GasPrice {
    let versioned_constants =
        orchestrator_versioned_constants::VersionedConstants::latest_constants();
    assert!(
        gas_target < versioned_constants.max_block_size,
        "Gas target must be lower than max block size."
    );
    assert!(gas_target.0 > 0, "Gas target must be greater than zero.");
    assert!(
        versioned_constants.gas_price_max_change_denominator > 0,
        "Denominator constant must be greater than zero."
    );

    // If the current price is below the minimum, apply a gradual adjustment and return early.
    // This allows the price to increase by at most 1/MIN_GAS_PRICE_INCREASE_DENOMINATOR per block.
    if price < min_gas_price {
        let max_increase = price.0 / MIN_GAS_PRICE_INCREASE_DENOMINATOR;
        let adjusted = price.0 + max_increase;
        // Cap at min_gas_price to avoid overshooting
        let adjusted_price = adjusted.min(min_gas_price.0);
        info!(
            "Fee Market: Price {} below minimum gas price {}, adjusted price: {} )",
            price.0, min_gas_price.0, adjusted_price
        );
        return GasPrice(adjusted_price);
    }

    // Use U256 to avoid overflow, as multiplying a u128 by a u64 remains within U256 bounds.
    let gas_delta = U256::from(gas_used.0.abs_diff(gas_target.0));
    let gas_target_u256 = U256::from(gas_target.0);
    let price_u256 = U256::from(price.0);

    // Calculate price change by multiplying first, then dividing. This avoids the precision loss
    // that occurs when dividing before multiplying.
    let denominator =
        gas_target_u256 * U256::from(versioned_constants.gas_price_max_change_denominator);
    let price_change = (price_u256 * gas_delta) / denominator;

    let adjusted_price_u256 =
        if gas_used > gas_target { price_u256 + price_change } else { price_u256 - price_change };

    // Sanity check: ensure direction of change is correct
    assert!(
        gas_used > gas_target && adjusted_price_u256 >= price_u256
            || gas_used <= gas_target && adjusted_price_u256 <= price_u256
    );

    // Price should not realistically exceed u128::MAX, bound to avoid theoretical overflow.
    let adjusted_price = u128::try_from(adjusted_price_u256).unwrap_or(u128::MAX);
    GasPrice(max(adjusted_price, min_gas_price.0))
}
