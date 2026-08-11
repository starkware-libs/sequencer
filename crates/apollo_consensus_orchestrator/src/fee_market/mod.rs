use std::cmp::{max, min};

use apollo_consensus_orchestrator_config::config::PricePerHeight;
use apollo_versioned_constants::VersionedConstants;
use ethnum::U256;
use serde::Serialize;
use starknet_api::block::{BlockNumber, GasPrice};
use starknet_api::execution_resources::GasAmount;
use starknet_api::versioned_constants_logic::VersionedConstantsTrait;
use tracing::info;

use crate::metrics::{record_l2_gas_price_clamped, L2GasPriceClampBound};

#[cfg(test)]
mod test;

// Denominator for the maximum gas price increase per block when price is below minimum.
// This controls how quickly the gas price can rise towards the minimum.
//
// With a denominator of 333: Each block can increase by at most 0.3% of the current price, to
// double the price takes approximately 230 blocks (at 2.6 seconds per block), this means doubling
// in approximately 10 minutes.
const MIN_GAS_PRICE_INCREASE_DENOMINATOR: u128 = 333;

// Ceiling on the L2 gas price, as a multiple of the minimum gas price in force for the height.
// TODO(asaf-sw): Move to versioned constants in 0.14.4.
const MAX_GAS_PRICE_MULTIPLIER: u128 = 10;
// A multiplier of 0 or 1 pins the L2 gas price at 0 or at the minimum.
const _: () = assert!(MAX_GAS_PRICE_MULTIPLIER > 1);

/// Fee market information for the next block.
#[cfg_attr(any(feature = "testing", test), derive(serde::Deserialize, PartialEq))]
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
    let fallback_min_gas_price = VersionedConstants::latest_constants().min_gas_price;
    min_l2_gas_price_per_height
        .iter()
        .rev()
        .find(|e| e.height <= height.0)
        .map(|e| GasPrice(e.price))
        .unwrap_or(fallback_min_gas_price)
}

/// The ceiling on the L2 gas price: `MAX_GAS_PRICE_MULTIPLIER` times `min_gas_price`. Applies to
/// every block regardless of its `starknet_version`. Proposer and validator reach the ceiling
/// through this function, so they cannot disagree on it.
pub fn l2_gas_price_cap(min_gas_price: GasPrice) -> GasPrice {
    GasPrice(min_gas_price.0.saturating_mul(MAX_GAS_PRICE_MULTIPLIER))
}

/// Compute the next L2 gas price (for the fin or for updating state). Respects override when set.
/// Reporting the bounds is the caller's job, through [`NextL2GasPrice::record_clamping`].
pub fn calculate_next_l2_gas_price_for_fin(
    current_l2_gas_price: GasPrice,
    height: BlockNumber,
    l2_gas_used: GasAmount,
    override_l2_gas_price_fri: Option<u128>,
    min_l2_gas_price_per_height: &[PricePerHeight],
    fee_actual: Option<GasPrice>,
) -> NextL2GasPrice {
    if let Some(override_value) = override_l2_gas_price_fri {
        // Operator pin: escapes both bounds by design; each side substitutes its own override.
        info!(
            "L2 gas price ({}) is not updated, remains on override value of {override_value} fri",
            current_l2_gas_price.0
        );
        return NextL2GasPrice { published_price: GasPrice(override_value), bounds: None };
    }
    let gas_target = VersionedConstants::latest_constants().gas_target;
    let config_min = get_min_gas_price_for_height(height, min_l2_gas_price_per_height);
    let cap = l2_gas_price_cap(config_min);

    let snip35_min = fee_actual.map_or(config_min, |fee_actual| max(config_min, fee_actual));
    let effective_min = min(snip35_min, cap);

    let raw_price =
        calculate_next_base_gas_price(current_l2_gas_price, l2_gas_used, gas_target, effective_min);

    NextL2GasPrice {
        published_price: min(raw_price, cap),
        bounds: Some(L2GasPriceBounds {
            current_price: current_l2_gas_price,
            raw_price,
            snip35_min,
            effective_min,
            cap,
        }),
    }
}

/// The next L2 gas price, and the bounds that produced it.
#[derive(Debug)]
pub struct NextL2GasPrice {
    /// The price to publish in the fin and to carry into the next block.
    pub published_price: GasPrice,
    // `None` for an operator override, which escapes both bounds.
    bounds: Option<L2GasPriceBounds>,
}

// The bounds in force for a block, and the prices compared against them.
#[derive(Debug)]
struct L2GasPriceBounds {
    current_price: GasPrice,
    raw_price: GasPrice,
    snip35_min: GasPrice,
    effective_min: GasPrice,
    cap: GasPrice,
}

impl NextL2GasPrice {
    /// Logs and counts the bounds that shaped `published_price`. Call once per decided block;
    /// blocks obtained through sync are not counted.
    pub(crate) fn record_clamping(&self) {
        let Some(bounds) = &self.bounds else {
            return;
        };
        // The current price, not `raw_price`: the last block of the ramp toward the minimum reaches
        // the minimum exactly, and the minimum is what stopped it there.
        if bounds.current_price < bounds.effective_min {
            record_l2_gas_price_clamped(L2GasPriceClampBound::Minimum);
        }
        // A SNIP-35 floor above the ceiling is a ceiling hit too: `effective_min` clipped the floor
        // down before the EIP-1559 step, so `raw_price` cannot show it.
        let raw_price_above_cap = bounds.raw_price > bounds.cap;
        let snip35_min_above_cap = bounds.snip35_min > bounds.cap;
        if raw_price_above_cap || snip35_min_above_cap {
            info!(
                "Fee Market: maximum gas price {} applied (price {} above it: {}, SNIP-35 floor \
                 {} above it: {}), published price: {}",
                bounds.cap.0,
                bounds.raw_price.0,
                raw_price_above_cap,
                bounds.snip35_min.0,
                snip35_min_above_cap,
                self.published_price.0
            );
            record_l2_gas_price_clamped(L2GasPriceClampBound::Maximum);
        }
    }
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
    let versioned_constants = VersionedConstants::latest_constants();
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
