use std::sync::LazyLock;

use apollo_consensus_orchestrator_config::config::PricePerHeight;
use starknet_api::block::{BlockNumber, GasPrice};
use starknet_api::execution_resources::GasAmount;
use starknet_api::versioned_constants_logic::VersionedConstantsTrait;

use crate::fee_market::{
    calculate_next_base_gas_price,
    get_min_gas_price_for_height,
    MIN_GAS_PRICE_INCREASE_DENOMINATOR,
};
use crate::orchestrator_versioned_constants::VersionedConstants;

static VERSIONED_CONSTANTS: LazyLock<&VersionedConstants> =
    LazyLock::new(VersionedConstants::latest_constants);

use rstest::rstest;

const INIT_PRICE: GasPrice = GasPrice(30_000_000_000);

#[rstest]
#[case::high_congestion(
    GasAmount(VERSIONED_CONSTANTS.max_block_size.0 * 3 / 4),
    VERSIONED_CONSTANTS.max_block_size / 2,
    GasPrice(30312500000),
)]
#[case::low_congestion(
    VERSIONED_CONSTANTS.max_block_size / 4,
    VERSIONED_CONSTANTS.max_block_size / 2,
    GasPrice(29687500000),
)]
#[case::stable(
    VERSIONED_CONSTANTS.max_block_size / 2,
    VERSIONED_CONSTANTS.max_block_size / 2,
    INIT_PRICE
)]
#[case::high_congestion_80(
    GasAmount(VERSIONED_CONSTANTS.max_block_size.0 * 9 / 10),
    GasAmount(VERSIONED_CONSTANTS.max_block_size.0 * 4 / 5), // Gas target 80%
    GasPrice(30078125000)
)]
#[case::low_congestion_80(
    GasAmount(VERSIONED_CONSTANTS.max_block_size.0 / 4),
    GasAmount(VERSIONED_CONSTANTS.max_block_size.0 * 4 / 5), // Gas target 80%
    GasPrice(29570312500)
)]
#[case::stable_80(
    GasAmount(VERSIONED_CONSTANTS.max_block_size.0 * 4/5),
    GasAmount(VERSIONED_CONSTANTS.max_block_size.0 * 4/5), // Gas target 80%
    INIT_PRICE
)]
fn price_calculation_snapshot(
    #[case] gas_used: GasAmount,
    #[case] gas_target: GasAmount,
    #[case] expected: GasPrice,
) {
    let min_gas_price = VERSIONED_CONSTANTS.min_gas_price;
    let actual = calculate_next_base_gas_price(INIT_PRICE, gas_used, gas_target, min_gas_price);
    assert_eq!(actual, expected);
}

#[test]
fn test_gas_price_with_extreme_values() {
    let max_block_size = VERSIONED_CONSTANTS.max_block_size;
    let min_gas_price = VERSIONED_CONSTANTS.min_gas_price;

    let price = min_gas_price;
    let gas_target = max_block_size / 2;
    let gas_used = GasAmount(0);
    assert_eq!(
        calculate_next_base_gas_price(price, gas_used, gas_target, min_gas_price),
        min_gas_price
    );

    let price = min_gas_price;
    let gas_target = max_block_size / 2;
    let gas_used = max_block_size;
    assert!(
        calculate_next_base_gas_price(price, gas_used, gas_target, min_gas_price) > min_gas_price
    );
}

#[rstest]
#[case::extreme_price_zero_usage(GasAmount(0))]
#[case::extreme_price_full_usage(VERSIONED_CONSTANTS.max_block_size)]
fn price_does_not_overflow(#[case] gas_used: GasAmount) {
    let price = GasPrice(u128::from(u64::MAX));
    let gas_target = VERSIONED_CONSTANTS.max_block_size / 2;
    let min_gas_price = VERSIONED_CONSTANTS.min_gas_price;

    // Should not panic.
    let _ = calculate_next_base_gas_price(price, gas_used, gas_target, min_gas_price);
}

#[test]
fn versioned_constants_gas_target_is_valid() {
    // Arbitrary values.
    let price = INIT_PRICE;
    let gas_used = GasAmount(100);
    let min_gas_price = VERSIONED_CONSTANTS.min_gas_price;

    // If panics, VersionedConstants::gas_target is not set correctly.
    calculate_next_base_gas_price(price, gas_used, VERSIONED_CONSTANTS.gas_target, min_gas_price);
}

#[test]
fn test_get_min_gas_price_for_height_exact_match() {
    let min_l2_gas_price_per_height = vec![
        PricePerHeight { height: 100, price: 10_000_000_000 },
        PricePerHeight { height: 500, price: 20_000_000_000 },
        PricePerHeight { height: 1000, price: 30_000_000_000 },
    ];

    // Exact match
    assert_eq!(
        get_min_gas_price_for_height(BlockNumber(500), &min_l2_gas_price_per_height),
        GasPrice(20_000_000_000)
    );
}

#[test]
fn test_get_min_gas_price_for_height_between_entries() {
    let min_l2_gas_price_per_height = vec![
        PricePerHeight { height: 100, price: 10_000_000_000 },
        PricePerHeight { height: 500, price: 20_000_000_000 },
        PricePerHeight { height: 1000, price: 30_000_000_000 },
    ];

    // Between 100 and 500, should use 100's price
    assert_eq!(
        get_min_gas_price_for_height(BlockNumber(300), &min_l2_gas_price_per_height),
        GasPrice(10_000_000_000)
    );

    // Between 500 and 1000, should use 500's price
    assert_eq!(
        get_min_gas_price_for_height(BlockNumber(750), &min_l2_gas_price_per_height),
        GasPrice(20_000_000_000)
    );
}

#[test]
fn test_get_min_gas_price_for_height_before_first_entry() {
    let min_l2_gas_price_per_height = vec![
        PricePerHeight { height: 100, price: 10_000_000_000 },
        PricePerHeight { height: 500, price: 20_000_000_000 },
    ];

    // Before first entry, should use fallback (versioned constants min_gas_price)
    let fallback_min_gas_price = VersionedConstants::latest_constants().min_gas_price;
    assert_eq!(
        get_min_gas_price_for_height(BlockNumber(50), &min_l2_gas_price_per_height),
        fallback_min_gas_price
    );
}

#[test]
fn test_get_min_gas_price_for_height_after_last_entry() {
    let min_l2_gas_price_per_height = vec![
        PricePerHeight { height: 100, price: 10_000_000_000 },
        PricePerHeight { height: 500, price: 20_000_000_000 },
        PricePerHeight { height: 1000, price: 30_000_000_000 },
    ];

    // After last entry, should use last entry's price
    assert_eq!(
        get_min_gas_price_for_height(BlockNumber(2000), &min_l2_gas_price_per_height),
        GasPrice(30_000_000_000)
    );
}

#[test]
fn test_get_min_gas_price_for_height_empty_list() {
    let min_l2_gas_price_per_height = vec![];

    // Empty list, should use fallback (versioned constants min_gas_price)
    let fallback_min_gas_price = VersionedConstants::latest_constants().min_gas_price;
    assert_eq!(
        get_min_gas_price_for_height(BlockNumber(100), &min_l2_gas_price_per_height),
        fallback_min_gas_price
    );
}

#[test]
fn test_calculate_with_price_below_minimum() {
    let min_gas_price = GasPrice(20_000_000_000);
    let price = GasPrice(10_000_000_000); // Below minimum
    let gas_used = GasAmount(1000);
    let gas_target = GasAmount(2000);

    let result = calculate_next_base_gas_price(price, gas_used, gas_target, min_gas_price);

    // When price < min_gas_price, should apply gradual adjustment
    // Price increases by at most 1/MIN_GAS_PRICE_INCREASE_DENOMINATOR per block
    let max_increase = price.0 / MIN_GAS_PRICE_INCREASE_DENOMINATOR;
    let expected = price.0 + max_increase;
    assert_eq!(result, GasPrice(expected));

    // Verify the increase is gradual (about 0.3% for denominator=333)
    assert!(result.0 > price.0);
    assert!(result.0 < min_gas_price.0); // Should not jump to minimum immediately
}

#[test]
fn test_calculate_with_price_close_to_minimum() {
    let min_gas_price = GasPrice(10_000_000_000);
    let price = GasPrice(9_971_000_000); // Very close to minimum
    let gas_used = GasAmount(1000);
    let gas_target = GasAmount(2000);

    let result = calculate_next_base_gas_price(price, gas_used, gas_target, min_gas_price);

    // When price is close to minimum, should cap at min_gas_price to avoid overshooting
    assert_eq!(result, min_gas_price);
}
