use std::cmp::max;

#[cfg(test)]
#[path = "fee_market_test.rs"]
pub mod fee_market_test;

//  This constant is used to calculate the base gas price for the next block according to EIP-1559
// and serves as a sensitivity parameter that limits the maximum rate of change of the gas price
// between consecutive blocks.
const GAS_PRICE_MAX_CHANGE_DENOMINATOR: u128 = 48;
const MIN_GAS_PRICE: u64 = 100000; // In fri.
// TODO(Mohammad): Check the exact value for maximum block size in StarkNet.
const MAX_BLOCK_SIZE: u64 = 4000000000; // In gas units. It's equivalent to 40M gas steps, with 100 gas units per step.

/// Calculate the base gas price for the next block according to EIP-1559.
///
/// # Parameters
/// - `price`: The base fee of the current block.
/// - `gas_used`: The total gas used in the current block.
/// - `gas_target`: The target gas usage per block (usually half of the gas limit).
pub fn calculate_next_base_gas_price(price: u64, gas_used: u64, gas_target: u64) -> u64 {
    // Setting the target at 50% of the max block size balances the rate of gas price changes,
    // helping to prevent sudden spikes, particularly during increases, for a better user
    // experience.
    assert_eq!(
        gas_target,
        MAX_BLOCK_SIZE / 2,
        "Gas target must be 50% of max block size to balance price changes."
    );
    // To prevent precision loss during multiplication and division, we set a minimum gas price.
    // Additionally, a minimum gas price is established to prevent prolonged periods before the
    // price reaches a higher value.
    assert!(
        price >= MIN_GAS_PRICE,
        "The gas price must be at least the minimum to prevent precision loss during \
         multiplication and division."
    );

    // We use unsigned integers (u64 and u128) to avoid overflow issues, as the input values are
    // naturally unsigned and i256 is unstable in Rust. This approach allows safe handling of
    // all inputs using u128 for intermediate calculations.

    // The absolute difference between gas_used and gas_target is always u64.
    let gas_delta = gas_used.abs_diff(gas_target);
    // Convert to u128 to prevent overflow, as a product of two u64 fits inside a u128.
    let price_u128 = u128::from(price);
    let gas_delta_u128 = u128::from(gas_delta);
    let gas_target_u128 = u128::from(gas_target);

    // Calculate the gas change as u128 to handle potential overflow during multiplication.
    let gas_delta_cost =
        price_u128.checked_mul(gas_delta_u128).expect("Both variables originate from u64");
    // Calculate the price change, maintaining precision by dividing after scaling up.
    // This avoids significant precision loss that would occur if dividing before
    // multiplication.
    let price_change_u128 = gas_delta_cost / gas_target_u128 / GAS_PRICE_MAX_CHANGE_DENOMINATOR;

    // Convert back to u64, as the price change should fit within the u64 range.
    // Since the target is half the maximum block size (which fits within a u64), the gas delta
    // is bounded by half the maximum block size. Therefore, after dividing by the gas target
    // (which is half the maximum block size), the result is guaranteed to fit within a u64.
    let price_change = u64::try_from(price_change_u128)
        .expect("Result fits u64 after division of a bounded gas delta");

    let adjusted_price =
        if gas_used > gas_target { price + price_change } else { price - price_change };

    assert!(
        gas_used > gas_target && adjusted_price >= price
            || gas_used <= gas_target && adjusted_price <= price
    );

    max(adjusted_price, MIN_GAS_PRICE)
}
