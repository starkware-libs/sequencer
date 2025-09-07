use std::sync::LazyLock;

use starknet_api::block::GasPrice;
use starknet_api::execution_resources::GasAmount;

use crate::fee_market::calculate_next_base_gas_price;
use crate::orchestrator_versioned_constants::VersionedConstants;

static VERSIONED_CONSTANTS: LazyLock<&VersionedConstants> =
    LazyLock::new(VersionedConstants::latest_constants);

use rstest::rstest;

const INIT_PRICE_FOR_TESTING: GasPrice = GasPrice(30_000_000_000);

#[rstest]
#[case::high_congestion(
    GasAmount(VERSIONED_CONSTANTS.max_block_size.0 * 3 / 4),
    VERSIONED_CONSTANTS.max_block_size / 2,
    GasPrice(INIT_PRICE_FOR_TESTING.0 + (INIT_PRICE_FOR_TESTING.0 / (VERSIONED_CONSTANTS.gas_price_max_change_denominator * 2)))
)]
#[case::low_congestion(
    VERSIONED_CONSTANTS.max_block_size / 4,
    VERSIONED_CONSTANTS.max_block_size / 2,
    GasPrice(INIT_PRICE_FOR_TESTING.0 - (INIT_PRICE_FOR_TESTING.0 / (VERSIONED_CONSTANTS.gas_price_max_change_denominator * 2)))
)]
#[case::stable(
    VERSIONED_CONSTANTS.max_block_size / 2,
    VERSIONED_CONSTANTS.max_block_size / 2,
    GasPrice(INIT_PRICE_FOR_TESTING.0)
)]
#[case::high_congestion_80(
    GasAmount(VERSIONED_CONSTANTS.max_block_size.0 * 9 / 10),
    GasAmount(VERSIONED_CONSTANTS.max_block_size.0 * 4 / 5), // Gas target 80%
    GasPrice(
        INIT_PRICE_FOR_TESTING.0
            + (INIT_PRICE_FOR_TESTING.0
                * u128::from(VERSIONED_CONSTANTS.max_block_size.0 / 10) // delta = |0.9*max - 0.8*max| = 0.1*max
                / (u128::from(VERSIONED_CONSTANTS.max_block_size.0 * 4 / 5)
                    * VERSIONED_CONSTANTS.gas_price_max_change_denominator)),
    )
)]
#[case::low_congestion_80(
    GasAmount(VERSIONED_CONSTANTS.max_block_size.0 / 4),
    GasAmount(VERSIONED_CONSTANTS.max_block_size.0 * 4 / 5), // Gas target 80%
    GasPrice(
        INIT_PRICE_FOR_TESTING.0
            - (INIT_PRICE_FOR_TESTING.0
                * u128::from(VERSIONED_CONSTANTS.max_block_size.0 * 11 / 20)) // delta = |0.25*max - 0.8*max| = 0.55*max
                / (u128::from(VERSIONED_CONSTANTS.max_block_size.0 * 4 / 5)
                    * VERSIONED_CONSTANTS.gas_price_max_change_denominator),
    )
)]
#[case::stable_80(
    GasAmount(VERSIONED_CONSTANTS.max_block_size.0 * 4/5),
    GasAmount(VERSIONED_CONSTANTS.max_block_size.0 * 4/5), // Gas target 80%
    GasPrice(INIT_PRICE_FOR_TESTING.0)
)]
fn price_calculation_snapshot(
    #[case] gas_used: GasAmount,
    #[case] gas_target: GasAmount,
    #[case] expected: GasPrice,
) {
    let actual = calculate_next_base_gas_price(INIT_PRICE_FOR_TESTING, gas_used, gas_target);
    assert_eq!(actual, expected);
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

#[test]
fn versioned_constants_gas_target_is_valid() {
    // Arbitrary values.
    let price = INIT_PRICE_FOR_TESTING;
    let gas_used = GasAmount(100);

    // If panics, VersionedConstants::gas_target is not set correctly.
    calculate_next_base_gas_price(price, gas_used, VERSIONED_CONSTANTS.gas_target);
}
