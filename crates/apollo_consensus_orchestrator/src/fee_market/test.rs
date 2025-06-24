use std::sync::LazyLock;

use starknet_api::block::GasPrice;
use starknet_api::execution_resources::GasAmount;

use crate::fee_market::calculate_next_base_gas_price;
use crate::orchestrator_versioned_constants::VersionedConstants;

static VERSIONED_CONSTANTS: LazyLock<&VersionedConstants> =
    LazyLock::new(VersionedConstants::latest_constants);

#[test]
fn test_price_calculation_snapshot() {
    // Setup: using realistic arbitrary values.
    let init_price = GasPrice(1_000_000_000);
    let max_block_size = VERSIONED_CONSTANTS.max_block_size;
    let gas_target = max_block_size / 2;
    let high_congestion_gas_used = GasAmount(max_block_size.0 * 3 / 4);
    let low_congestion_gas_used = max_block_size / 4;
    let stable_congestion_gas_used = gas_target;

    // Fixed expected output values.
    let increased_price = GasPrice(init_price.0 + 10416666); // 1000000000 + (1000000000 * 1 / 4 * max_block_size) / (0.5 * max_block_size * 48);
    let decreased_price = GasPrice(init_price.0 - 10416666); // 1000000000 - (1000000000 * 1 / 4 * max_block_size) / (0.5 * max_block_size * 48);

    // Assert.
    assert_eq!(
        calculate_next_base_gas_price(init_price, high_congestion_gas_used, gas_target),
        increased_price
    );
    assert_eq!(
        calculate_next_base_gas_price(init_price, low_congestion_gas_used, gas_target),
        decreased_price
    );
    assert_eq!(
        calculate_next_base_gas_price(init_price, stable_congestion_gas_used, gas_target),
        init_price
    );
}

#[test]
// This test ensures that the gas price calculation does not overflow with extreme values,
fn test_gas_price_with_extreme_values() {
    let max_block_size = VERSIONED_CONSTANTS.max_block_size;
    let min_gas_price = VERSIONED_CONSTANTS.min_gas_price;
    let gas_price_max_change_denominator = VERSIONED_CONSTANTS.gas_price_max_change_denominator;

    let price = min_gas_price;
    let gas_target = max_block_size / 2;
    let gas_used = GasAmount(0);
    assert_eq!(calculate_next_base_gas_price(price, gas_used, gas_target), min_gas_price);

    let price = min_gas_price;
    let gas_target = max_block_size / 2;
    let gas_used = max_block_size;
    assert!(calculate_next_base_gas_price(price, gas_used, gas_target) > min_gas_price);

    let price = GasPrice(u128::from(u64::MAX));
    let gas_target = max_block_size / 2;
    let gas_used = GasAmount(0);
    calculate_next_base_gas_price(price, gas_used, gas_target); // Should not panic.

    // To avoid overflow when updating the price, the value is set below a certain threshold so that
    // the new price does not exceed u64::MAX.
    let max_u128 = u128::from(u64::MAX);
    let calculated_price = GasPrice(
        max_u128 * gas_price_max_change_denominator / (gas_price_max_change_denominator + 1),
    );
    let gas_target = max_block_size / 2;
    let gas_used = max_block_size;
    calculate_next_base_gas_price(calculated_price, gas_used, gas_target); // Should not panic.
}
