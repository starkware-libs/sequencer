use std::cmp::max;

use apollo_consensus_orchestrator_config::config::PricePerHeight;
use ethnum::U256;
use serde::Serialize;
use starknet_api::block::{BlockNumber, GasPrice};
use starknet_api::execution_resources::GasAmount;
use starknet_api::versioned_constants_logic::VersionedConstantsTrait;
use tracing::info;

use crate::orchestrator_versioned_constants;

#[cfg(test)]
mod test;

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
    // This allows the price to increase by at most 1/gas_price_max_change_denominator per block.
    if price < min_gas_price {
        let max_increase = price.0 / versioned_constants.gas_price_max_change_denominator;
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
