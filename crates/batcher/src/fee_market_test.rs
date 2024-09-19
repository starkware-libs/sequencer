use crate::fee_market::{
    calculate_next_base_gas_price,
    GAS_PRICE_MAX_CHANGE_DENOMINATOR,
    MAX_BLOCK_SIZE,
    MIN_GAS_PRICE,
};

#[test]
fn test_price_calculation_snapshot() {
    // Setup: using realistic arbitrary values.
    const INIT_PRICE: u64 = 1_000_000;
    const GAS_TARGET: u64 = MAX_BLOCK_SIZE / 2;
    const HIGH_CONGESTION_GAS_USED: u64 = MAX_BLOCK_SIZE * 3 / 4;
    const LOW_CONGESTION_GAS_USED: u64 = MAX_BLOCK_SIZE / 4;
    const STABLE_CONGESTION_GAS_USED: u64 = GAS_TARGET;

    // Fixed expected output values.
    let increased_price = 1000000 + 10416; // 1000000 + (1000000 * 1 / 4 * MAX_BLOCK_SIZE) / (0.5 * MAX_BLOCK_SIZE * 48);
    let decreased_price = 1000000 - 10416; // 1000000 - (1000000 * 1 / 4 * MAX_BLOCK_SIZE) / (0.5 * MAX_BLOCK_SIZE * 48);

    // Assert.
    assert_eq!(
        calculate_next_base_gas_price(INIT_PRICE, HIGH_CONGESTION_GAS_USED, GAS_TARGET),
        increased_price
    );
    assert_eq!(
        calculate_next_base_gas_price(INIT_PRICE, LOW_CONGESTION_GAS_USED, GAS_TARGET),
        decreased_price
    );
    assert_eq!(
        calculate_next_base_gas_price(INIT_PRICE, STABLE_CONGESTION_GAS_USED, GAS_TARGET),
        INIT_PRICE
    );
}

#[test]
// This test ensures that the gas price calculation does not overflow with extreme values,
fn test_gas_price_with_extreme_values() {
    let price = MIN_GAS_PRICE;
    let gas_target = MAX_BLOCK_SIZE / 2;
    let gas_used = 0;
    assert_eq!(calculate_next_base_gas_price(price, gas_used, gas_target), MIN_GAS_PRICE);

    let price = MIN_GAS_PRICE;
    let gas_target = MAX_BLOCK_SIZE / 2;
    let gas_used = MAX_BLOCK_SIZE;
    assert!(calculate_next_base_gas_price(price, gas_used, gas_target) > MIN_GAS_PRICE);

    let price = u64::MAX;
    let gas_target = MAX_BLOCK_SIZE / 2;
    let gas_used = 0;
    calculate_next_base_gas_price(price, gas_used, gas_target); // Should not panic.

    // To avoid overflow when updating the price, the value is set below a certain threshold so that
    // the new price does not exceed u64::MAX.
    let max_u128 = u128::from(u64::MAX);
    let price_u128 =
        max_u128 * GAS_PRICE_MAX_CHANGE_DENOMINATOR / (GAS_PRICE_MAX_CHANGE_DENOMINATOR + 1);
    let gas_target = MAX_BLOCK_SIZE / 2;
    let gas_used = MAX_BLOCK_SIZE;
    calculate_next_base_gas_price(u64::try_from(price_u128).unwrap(), gas_used, gas_target);
    // Should not panic.
}
