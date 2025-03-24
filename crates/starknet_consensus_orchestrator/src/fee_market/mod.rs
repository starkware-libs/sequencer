use std::cmp::max;

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
    let versioned_constants =
        orchestrator_versioned_constants::VersionedConstants::latest_constants();
    // Setting the target at 50% of the max block size balances the rate of gas price changes,
    // helping to prevent sudden spikes, particularly during increases, for a better user
    // experience.
    assert_eq!(
        gas_target,
        versioned_constants.max_block_size / 2,
        "Gas target must be 50% of max block size to balance price changes."
    );
    // To prevent precision loss during multiplication and division, we set a minimum gas price.
    // Additionally, a minimum gas price is established to prevent prolonged periods before the
    // price reaches a higher value.
    assert!(
        price >= versioned_constants.min_gas_price,
        "The gas price must be at least the minimum to prevent precision loss during \
         multiplication and division."
    );

    // We use unsigned integers (u64 and u128) to avoid overflow issues, as the input values are
    // naturally unsigned and i256 is unstable in Rust. This approach allows safe handling of
    // all inputs using u128 for intermediate calculations.

    // The absolute difference between gas_used and gas_target is always u64.
    let gas_delta = gas_used.0.abs_diff(gas_target.0);
    // Convert to u128 to prevent overflow, as a product of two u64 fits inside a u128.
    let gas_delta_u128 = gas_delta.into();
    let gas_target_u128: u128 = gas_target.0.into();

    let gas_delta_cost =
        price.0.checked_mul(gas_delta_u128).expect("Multiplication overflow detected");
    // Calculate the price change, maintaining precision by dividing after scaling up.
    // This avoids significant precision loss that would occur if dividing before
    // multiplication.
    let price_change = gas_delta_cost
        .checked_div(gas_target_u128 * versioned_constants.gas_price_max_change_denominator)
        .expect("Division error, denominator must be nonzero");

    let adjusted_price =
        if gas_used > gas_target { price.0 + price_change } else { price.0 - price_change };

    assert!(
        gas_used > gas_target && adjusted_price >= price.0
            || gas_used <= gas_target && adjusted_price <= price.0
    );

    GasPrice(max(adjusted_price, versioned_constants.min_gas_price.0))
}
