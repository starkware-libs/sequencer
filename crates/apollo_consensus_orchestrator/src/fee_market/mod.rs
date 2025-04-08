use std::cmp::max;

use alloy::primitives::U256;
use serde::Serialize;
use starknet_api::block::GasPrice;
use starknet_api::execution_resources::GasAmount;

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

/// Calculate the base gas price for the next block according to EIP-1559.
///
/// # Parameters
/// - `price`: The base gas price per unit (in fri) of the current block.
/// - `gas_used`: The total gas used in the current block.
/// - `gas_target`: The target gas usage per block (usually half of a block's gas limit).
pub fn calculate_next_base_gas_price(
    price: GasPrice,
    gas_used: GasAmount,
    gas_target: GasAmount,
) -> GasPrice {
    let constants = orchestrator_versioned_constants::VersionedConstants::latest_constants();
    // Setting target to 50% of max block size balances price changes and prevents spikes.
    assert_eq!(
        gas_target,
        constants.max_block_size / 2,
        "Gas target must be 50% of max block size to balance price changes."
    );
    // A minimum gas price prevents precision loss. Additionally, a minimum gas price helps avoid
    // extended periods of low pricing.
    assert!(
        price >= constants.min_gas_price,
        "The gas price must be at least the minimum to prevent precision loss."
    );

    // Use U256 to avoid overflow since u128 × u64 stays within U256 bounds.
    let gas_delta = U256::from(gas_used.0.abs_diff(gas_target.0));
    let gas_target_u256 = U256::from(gas_target.0);
    let price_u256 = U256::from(price.0);

    // Calculate price change by multiplying first, then dividing. This avoids the precision loss
    // that occurs when dividing before multiplying.
    let denominator = gas_target_u256 * U256::from(constants.gas_price_max_change_denominator);
    let price_change = (price_u256 * gas_delta) / denominator;

    let adjusted_price_u256 =
        if gas_used > gas_target { price_u256 + price_change } else { price_u256 - price_change };

    // Sanity check: ensure direction of change is correct
    assert!(
        gas_used > gas_target && adjusted_price_u256 >= price_u256
            || gas_used <= gas_target && adjusted_price_u256 <= price_u256
    );

    let adjusted_price: u128 = adjusted_price_u256.try_into().expect("Failed to convert to u128");
    GasPrice(max(adjusted_price, constants.min_gas_price.0))
}
