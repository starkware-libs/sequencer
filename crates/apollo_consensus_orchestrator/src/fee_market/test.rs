use std::sync::LazyLock;

use apollo_consensus_orchestrator_config::config::PricePerHeight;
use starknet_api::block::{BlockNumber, GasPrice};
use starknet_api::execution_resources::GasAmount;
use starknet_api::versioned_constants_logic::VersionedConstantsTrait;

use crate::fee_market::{
    calculate_next_base_gas_price,
    compute_fee_actual,
    compute_fee_proposal,
    compute_fee_target,
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

#[test]
fn test_compute_fee_actual_with_10_identical_values() {
    let proposals: Vec<GasPrice> = vec![GasPrice(1000); 10];
    assert_eq!(compute_fee_actual(&proposals, 10), Some(GasPrice(1000)));
}

#[test]
fn test_compute_fee_actual_with_ascending_values() {
    let proposals: Vec<GasPrice> = (1..=10).map(|i| GasPrice(i * 100)).collect();
    // Sorted: 100,200,300,400,500,600,700,800,900,1000. Median = (500+600)/2 = 550.
    assert_eq!(compute_fee_actual(&proposals, 10), Some(GasPrice(550)));
}

#[test]
fn test_compute_fee_actual_with_descending_values() {
    let proposals: Vec<GasPrice> = (1..=10).rev().map(|i| GasPrice(i * 100)).collect();
    // Same sorted order, same median.
    assert_eq!(compute_fee_actual(&proposals, 10), Some(GasPrice(550)));
}

#[test]
fn test_compute_fee_actual_with_outliers() {
    let mut proposals: Vec<GasPrice> = vec![GasPrice(100); 8];
    proposals.push(GasPrice(1)); // Low outlier
    proposals.push(GasPrice(1_000_000)); // High outlier
    // Sorted: 1,100,100,100,100,100,100,100,100,1000000. Median = (100+100)/2 = 100.
    assert_eq!(compute_fee_actual(&proposals, 10), Some(GasPrice(100)));
}

#[test]
fn test_compute_fee_actual_fewer_than_window_returns_none() {
    let proposals: Vec<GasPrice> = vec![GasPrice(100); 9];
    assert_eq!(compute_fee_actual(&proposals, 10), None);
}

#[test]
fn test_compute_fee_actual_empty_returns_none() {
    assert_eq!(compute_fee_actual(&[], 10), None);
}

#[test]
fn test_compute_fee_actual_custom_window_size() {
    let proposals: Vec<GasPrice> = vec![GasPrice(200); 4];
    assert_eq!(compute_fee_actual(&proposals, 4), Some(GasPrice(200)));
    assert_eq!(compute_fee_actual(&proposals, 5), None);
}

#[test]
fn test_compute_fee_actual_zero_median_returns_none() {
    // All zeros → median is 0 → returns None (triggers l2_gas_price fallback).
    let proposals: Vec<GasPrice> = vec![GasPrice(0); 10];
    assert_eq!(compute_fee_actual(&proposals, 10), None);
}

#[test]
fn test_compute_fee_actual_uses_only_last_window_entries() {
    // 12 entries: first 2 are outliers, last 10 are 500.
    let mut proposals: Vec<GasPrice> = vec![GasPrice(999_999); 2];
    proposals.extend(vec![GasPrice(500); 10]);
    // With window_size=10, the outliers should be ignored (only last 10 used).
    assert_eq!(compute_fee_actual(&proposals, 10), Some(GasPrice(500)));
}

#[test]
fn test_compute_fee_target_normal() {
    // Target: $3e-9/gas = 3_000_000_000 atto-USD. STRK at $0.50 = 500_000_000_000_000_000.
    // floor = 3_000_000_000 * 10^18 / 500_000_000_000_000_000 = 6_000_000_000.
    let target = compute_fee_target(3_000_000_000, 500_000_000_000_000_000, 0, u128::MAX);
    assert_eq!(target, GasPrice(6_000_000_000));
}

#[test]
fn test_compute_fee_target_clamp_min() {
    let target = compute_fee_target(1, 10u128.pow(18), 100, u128::MAX);
    // floor = 1 * 10^18 / 10^18 = 1, but clamped to min 100.
    assert_eq!(target, GasPrice(100));
}

#[test]
fn test_compute_fee_target_clamp_max() {
    // Very low STRK price → very high floor, clamped to max.
    let target = compute_fee_target(10u128.pow(18), 1, 0, 1000);
    assert_eq!(target, GasPrice(1000));
}

#[test]
fn test_compute_fee_target_zero_rate() {
    let target = compute_fee_target(100, 0, 0, 999);
    assert_eq!(target, GasPrice(999));
}

#[test]
fn test_compute_fee_proposal_oracle_failure_freezes() {
    let proposal = compute_fee_proposal(None, GasPrice(1000), 2);
    assert_eq!(proposal, GasPrice(1000));
}

#[test]
fn test_compute_fee_proposal_target_above_actual() {
    // fee_target=2000, fee_actual=1000, margin=2ppt. Upper bound = 1000*1002/1000 = 1002.
    let proposal = compute_fee_proposal(Some(GasPrice(2000)), GasPrice(1000), 2);
    assert_eq!(proposal, GasPrice(1002));
}

#[test]
fn test_compute_fee_proposal_target_below_actual() {
    // fee_target=500, fee_actual=1000, margin=2ppt. Lower bound = 1000*1000/1002 = 998.
    let proposal = compute_fee_proposal(Some(GasPrice(500)), GasPrice(1000), 2);
    assert_eq!(proposal, GasPrice(998));
}

#[test]
fn test_compute_fee_proposal_target_within_bounds() {
    // fee_target=1001, fee_actual=1000, margin=2ppt. 1001 is within [998, 1002].
    let proposal = compute_fee_proposal(Some(GasPrice(1001)), GasPrice(1000), 2);
    assert_eq!(proposal, GasPrice(1001));
}

#[test]
fn test_compute_fee_proposal_fee_actual_zero_clamps_to_zero() {
    // When fee_actual=0, both bounds are 0, so fee_proposal is always 0.
    let proposal = compute_fee_proposal(Some(GasPrice(1000)), GasPrice(0), 2);
    assert_eq!(proposal, GasPrice(0));
}

#[test]
fn test_compute_fee_actual_window_size_below_minimum_returns_none() {
    let proposals: Vec<GasPrice> = vec![GasPrice(100); 10];
    assert_eq!(compute_fee_actual(&proposals, 0), None);
    assert_eq!(compute_fee_actual(&proposals, 1), None);
}

#[test]
fn test_compute_fee_proposal_custom_margin() {
    // margin=10ppt (1%). fee_actual=10000. Upper=10000*1010/1000=10100. Lower=10000*1000/1010=9900.
    let proposal_up = compute_fee_proposal(Some(GasPrice(99999)), GasPrice(10000), 10);
    assert_eq!(proposal_up, GasPrice(10100));
    let proposal_down = compute_fee_proposal(Some(GasPrice(1)), GasPrice(10000), 10);
    assert_eq!(proposal_down, GasPrice(9900));
}
