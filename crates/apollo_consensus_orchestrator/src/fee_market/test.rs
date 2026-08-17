use std::sync::LazyLock;

use apollo_consensus_orchestrator_config::config::PricePerHeight;
use apollo_versioned_constants::VersionedConstants;
use metrics_exporter_prometheus::PrometheusBuilder;
use rstest::rstest;
use starknet_api::block::{BlockNumber, GasPrice, StarknetVersion};
use starknet_api::execution_resources::GasAmount;
use starknet_api::versioned_constants_logic::VersionedConstantsTrait;
use strum::IntoEnumIterator;

use crate::fee_market::{
    calculate_next_base_gas_price,
    calculate_next_l2_gas_price_for_fin,
    get_min_gas_price_for_height,
    l2_gas_price_cap,
    MIN_GAS_PRICE_INCREASE_DENOMINATOR,
};
use crate::metrics::{CONSENSUS_L2_GAS_PRICE_CLAMPED, LABEL_L2_GAS_PRICE_CLAMP_BOUND};
use crate::test_utils::{TEST_MAX_L2_GAS_PRICE, TEST_MIN_L2_GAS_PRICE};

static VERSIONED_CONSTANTS: LazyLock<&VersionedConstants> =
    LazyLock::new(VersionedConstants::latest_constants);

const INIT_PRICE: GasPrice = GasPrice(30_000_000_000);

// One entry from genesis, so the ceiling is `TEST_MAX_L2_GAS_PRICE` at every height.
fn flat_min_gas_price_config() -> Vec<PricePerHeight> {
    vec![PricePerHeight { height: 0, price: TEST_MIN_L2_GAS_PRICE.0 }]
}

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
fn test_ceiling_constants_match_the_shipped_multiplier() {
    assert_eq!(
        TEST_MAX_L2_GAS_PRICE.0,
        TEST_MIN_L2_GAS_PRICE.0 * VERSIONED_CONSTANTS.max_gas_price_multiplier
    );
}

#[test]
fn test_versioned_constants_max_gas_price_multiplier_is_valid() {
    // The assert inside `l2_gas_price_cap` is the check; a bad multiplier panics here.
    l2_gas_price_cap(BlockNumber(0), &flat_min_gas_price_config());
}

#[test]
fn test_every_version_with_versioned_constants_carries_the_latest_multiplier() {
    // The ceiling is ungated, so it binds blocks whose `starknet_version` predates it.
    let first_version = VersionedConstants::first_version();
    for version in StarknetVersion::iter() {
        let Ok(constants) = VersionedConstants::get(&version) else {
            // Only versions below the first one with versioned constants may lack a JSON entry.
            assert!(version < first_version, "Version {version} has no versioned constants.");
            continue;
        };
        assert_eq!(
            constants.max_gas_price_multiplier, VERSIONED_CONSTANTS.max_gas_price_multiplier,
            "Version {version} carries a different `max_gas_price_multiplier` than the latest \
             version."
        );
    }
}

#[test]
fn test_l2_gas_price_cap_tracks_the_per_height_minimum() {
    let min_l2_gas_price_per_height = vec![
        PricePerHeight { height: 100, price: 10_000_000_000 },
        PricePerHeight { height: 500, price: 20_000_000_000 },
    ];
    assert_eq!(
        l2_gas_price_cap(BlockNumber(300), &min_l2_gas_price_per_height),
        GasPrice(100_000_000_000)
    );
    assert_eq!(
        l2_gas_price_cap(BlockNumber(500), &min_l2_gas_price_per_height),
        GasPrice(200_000_000_000)
    );
}

#[test]
fn test_l2_gas_price_cap_saturates_at_an_extreme_configured_minimum() {
    // The minimum is deployment-configured, so an extreme value must saturate, not overflow.
    let min_l2_gas_price_per_height = vec![PricePerHeight { height: 0, price: u128::MAX }];
    assert_eq!(l2_gas_price_cap(BlockNumber(0), &min_l2_gas_price_per_height), GasPrice(u128::MAX));
}

#[test]
fn test_price_pinned_at_the_ceiling_follows_the_ceiling_down() {
    const STEP_HEIGHT: u64 = 500;
    let min_l2_gas_price_per_height = vec![
        PricePerHeight { height: 0, price: TEST_MIN_L2_GAS_PRICE.0 },
        PricePerHeight { height: STEP_HEIGHT, price: TEST_MIN_L2_GAS_PRICE.0 / 4 },
    ];
    let lowered_ceiling = GasPrice(TEST_MAX_L2_GAS_PRICE.0 / 4);

    // Just before the step the old ceiling still holds a congested price at 80 gwei.
    let before_the_step = calculate_next_l2_gas_price_for_fin(
        TEST_MAX_L2_GAS_PRICE,
        BlockNumber(STEP_HEIGHT - 1),
        VERSIONED_CONSTANTS.max_block_size,
        None,
        &min_l2_gas_price_per_height,
        None,
    );
    assert_eq!(before_the_step, TEST_MAX_L2_GAS_PRICE);

    let at_the_step = calculate_next_l2_gas_price_for_fin(
        TEST_MAX_L2_GAS_PRICE,
        BlockNumber(STEP_HEIGHT),
        VERSIONED_CONSTANTS.max_block_size,
        None,
        &min_l2_gas_price_per_height,
        None,
    );
    assert_eq!(at_the_step, lowered_ceiling);
}

#[rstest]
// Ordinary price inside the band: the counters must read 0, not "no data".
#[case::no_clamp(GasPrice(TEST_MIN_L2_GAS_PRICE.0 * 2), VERSIONED_CONSTANTS.gas_target, None, 0, 0)]
// Below the configured minimum: the floor binds and only the floor is counted.
#[case::below_the_minimum(
    GasPrice(TEST_MIN_L2_GAS_PRICE.0 / 2),
    VERSIONED_CONSTANTS.gas_target,
    None,
    1,
    0
)]
// A full block at the ceiling drives the EIP-1559 result above it: only the ceiling is counted.
#[case::above_the_ceiling(TEST_MAX_L2_GAS_PRICE, VERSIONED_CONSTANTS.max_block_size, None, 0, 1)]
// The SNIP-35 floor alone is above the ceiling: only the ceiling is counted.
#[case::snip35_floor_above_the_ceiling(
    TEST_MAX_L2_GAS_PRICE,
    VERSIONED_CONSTANTS.gas_target,
    Some(GasPrice(TEST_MAX_L2_GAS_PRICE.0 * 25)),
    0,
    1
)]
// Below the ceiling-clamped SNIP-35 floor: one block counts both bounds.
#[case::both_bounds_on_one_block(
    GasPrice(TEST_MIN_L2_GAS_PRICE.0 / 2),
    VERSIONED_CONSTANTS.gas_target,
    Some(GasPrice(TEST_MAX_L2_GAS_PRICE.0 * 25)),
    1,
    1
)]
fn test_l2_gas_price_clamp_counters(
    #[case] current_l2_gas_price: GasPrice,
    #[case] l2_gas_used: GasAmount,
    #[case] fee_actual: Option<GasPrice>,
    #[case] expected_minimum_count: u64,
    #[case] expected_maximum_count: u64,
) {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    CONSENSUS_L2_GAS_PRICE_CLAMPED.register();

    calculate_next_l2_gas_price_for_fin(
        current_l2_gas_price,
        BlockNumber(0),
        l2_gas_used,
        None,
        &flat_min_gas_price_config(),
        fee_actual,
    );

    // Label values are the dashboard and alert contract, so assert them literally.
    let metrics = recorder.handle().render();
    CONSENSUS_L2_GAS_PRICE_CLAMPED.assert_eq(
        &metrics,
        expected_minimum_count,
        &[(LABEL_L2_GAS_PRICE_CLAMP_BOUND, "minimum")],
    );
    CONSENSUS_L2_GAS_PRICE_CLAMPED.assert_eq(
        &metrics,
        expected_maximum_count,
        &[(LABEL_L2_GAS_PRICE_CLAMP_BOUND, "maximum")],
    );
}

#[test]
fn test_sustained_congestion_stops_at_the_ceiling() {
    // A full block drives the EIP-1559 price up ~9.5%, independently of the oracle.
    let mut price = TEST_MIN_L2_GAS_PRICE;

    for height in 0..100 {
        price = calculate_next_l2_gas_price_for_fin(
            price,
            BlockNumber(height),
            VERSIONED_CONSTANTS.max_block_size,
            None,
            &flat_min_gas_price_config(),
            None,
        );
    }

    // Exact equality, not `price <= cap`, which would also pass for a price that never rose.
    assert_eq!(price, TEST_MAX_L2_GAS_PRICE);
}

#[test]
fn test_fee_actual_above_the_ceiling_publishes_the_ceiling() {
    // `fee_actual` enters as a floor, and the floor is itself clamped to the ceiling.
    let price = calculate_next_l2_gas_price_for_fin(
        TEST_MAX_L2_GAS_PRICE,
        BlockNumber(0),
        VERSIONED_CONSTANTS.gas_target,
        None,
        &flat_min_gas_price_config(),
        Some(GasPrice(TEST_MAX_L2_GAS_PRICE.0 * 25)),
    );

    assert_eq!(price, TEST_MAX_L2_GAS_PRICE);
}

#[rstest]
#[case::override_above_the_ceiling(GasPrice(TEST_MAX_L2_GAS_PRICE.0 * 100))]
#[case::override_below_the_floor(GasPrice(TEST_MIN_L2_GAS_PRICE.0 / 100))]
fn test_override_bypasses_both_bounds(#[case] override_l2_gas_price_fri: GasPrice) {
    assert_eq!(
        calculate_next_l2_gas_price_for_fin(
            TEST_MIN_L2_GAS_PRICE,
            BlockNumber(0),
            VERSIONED_CONSTANTS.gas_target,
            Some(override_l2_gas_price_fri.0),
            &flat_min_gas_price_config(),
            None,
        ),
        override_l2_gas_price_fri
    );
}

#[test]
fn test_ceiling_leaves_ordinary_prices_untouched() {
    let price = GasPrice(TEST_MIN_L2_GAS_PRICE.0 * 2);
    let gas_used = GasAmount(VERSIONED_CONSTANTS.gas_target.0 * 3 / 2);

    let capped = calculate_next_l2_gas_price_for_fin(
        price,
        BlockNumber(0),
        gas_used,
        None,
        &flat_min_gas_price_config(),
        None,
    );

    assert_eq!(
        capped,
        calculate_next_base_gas_price(
            price,
            gas_used,
            VERSIONED_CONSTANTS.gas_target,
            TEST_MIN_L2_GAS_PRICE
        )
    );
    assert!(capped > price);
}
