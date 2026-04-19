use std::collections::BTreeMap;

use rstest::rstest;
use starknet_api::block::{BlockNumber, GasPrice};

use crate::snip35::{compute_fee_actual, compute_fee_proposal, compute_fee_target};

const FRI_DECIMALS_SCALE: u128 = 10u128.pow(18);
const FLOOR_MIN_FRI: u128 = 100;
const FLOOR_MAX_FRI: u128 = 1000;

fn window_from(entries: impl IntoIterator<Item = (u64, Option<GasPrice>)>) -> BTreeMap<BlockNumber, Option<GasPrice>> {
    entries.into_iter().map(|(h, v)| (BlockNumber(h), v)).collect()
}

#[test]
fn test_compute_fee_actual_random_window() {
    // The window for height 12 is heights [2, 11]; including heights 0..1 would shift the
    // median.
    // Sorted window: [1, 17, 42, 87, 100, 271, 999, 1024, 6000, 9999].
    // Even-length median = sorted[4] + (sorted[5] - sorted[4]) / 2 = 100 + 85 = 185.
    //
    // Buggy alternatives (for catching off-by-one or wrong-end-of-window bugs):
    // - Heights 0..9 instead of 2..11: sorted =
    //   [1, 42, 87, 100, 271, 314, 1024, 1729, 6000, 9999], median = 271 + (314 - 271) / 2 = 292.
    // - All 12 (window-size ignored): sorted[5..7] = [271, 314], median = 292.
    let values = [
        GasPrice(314),  // height 0
        GasPrice(1729), // height 1
        GasPrice(42),
        GasPrice(1024),
        GasPrice(100),
        GasPrice(9999),
        GasPrice(87),
        GasPrice(271),
        GasPrice(1),
        GasPrice(6000),
        GasPrice(17),
        GasPrice(999), // height 11
    ];
    let window =
        window_from(values.iter().enumerate().map(|(h, v)| (u64::try_from(h).unwrap(), Some(*v))));
    assert_eq!(compute_fee_actual(&window, BlockNumber(12)), Some(GasPrice(185)));
}

#[test]
fn test_compute_fee_actual_missing_entry_returns_none() {
    // Heights [0, 9] needed for fee_actual at height 10; height 5 is missing.
    let window = window_from(
        (0u64..10).filter(|h| *h != 5).map(|h| (h, Some(GasPrice(100)))),
    );
    assert_eq!(compute_fee_actual(&window, BlockNumber(10)), None);
}

#[test]
fn test_compute_fee_actual_none_entry_returns_none() {
    // Heights [0, 9] needed; height 7 is recorded as None (pre-SNIP-35 block).
    let window = window_from((0u64..10).map(|h| (h, if h == 7 { None } else { Some(GasPrice(100)) })));
    assert_eq!(compute_fee_actual(&window, BlockNumber(10)), None);
}

#[test]
fn test_compute_fee_actual_height_below_window_size_returns_none() {
    let window = window_from((0u64..5).map(|h| (h, Some(GasPrice(100)))));
    assert_eq!(compute_fee_actual(&window, BlockNumber(5)), None);
}

#[rstest]
// Target: $3e-9/gas = 3_000_000_000 atto-USD. STRK at $0.50 = 500_000_000_000_000_000.
// floor = 3_000_000_000 * 10^18 / 500_000_000_000_000_000 = 6_000_000_000.
#[case::normal(3_000_000_000, 500_000_000_000_000_000, 0, u128::MAX, GasPrice(6_000_000_000))]
// floor = 1 * 10^18 / 10^18 = 1, clamped up to FLOOR_MIN_FRI.
#[case::clamp_min(1, FRI_DECIMALS_SCALE, FLOOR_MIN_FRI, u128::MAX, GasPrice(FLOOR_MIN_FRI))]
// Very low STRK price → very high floor, clamped down to FLOOR_MAX_FRI.
#[case::clamp_max(FRI_DECIMALS_SCALE, 1, 0, FLOOR_MAX_FRI, GasPrice(FLOOR_MAX_FRI))]
#[case::zero_rate_returns_max(FRI_DECIMALS_SCALE, 0, 0, FLOOR_MAX_FRI, GasPrice(FLOOR_MAX_FRI))]
fn test_compute_fee_target(
    #[case] target_atto_usd_per_l2_gas: u128,
    #[case] strk_usd_rate: u128,
    #[case] floor_min_fri: u128,
    #[case] floor_max_fri: u128,
    #[case] expected: GasPrice,
) {
    let actual =
        compute_fee_target(target_atto_usd_per_l2_gas, strk_usd_rate, floor_min_fri, floor_max_fri);
    assert_eq!(actual, expected);
}

#[rstest]
#[case::oracle_failure_freezes_at_actual(None, GasPrice(1000), 2, GasPrice(1000))]
// margin=2ppt → bounds = [1000*1000/1002, 1000*1002/1000] = [998, 1002].
#[case::target_above_actual_clamps_to_upper(
    Some(GasPrice(2000)),
    GasPrice(1000),
    2,
    GasPrice(1002)
)]
#[case::target_below_actual_clamps_to_lower(Some(GasPrice(500)), GasPrice(1000), 2, GasPrice(998))]
#[case::target_within_bounds_above_actual(Some(GasPrice(1001)), GasPrice(1000), 2, GasPrice(1001))]
#[case::target_within_bounds_below_actual(Some(GasPrice(999)), GasPrice(1000), 2, GasPrice(999))]
// When fee_actual=0, both bounds are 0, so fee_proposal is always 0.
#[case::fee_actual_zero_clamps_to_zero(Some(GasPrice(1000)), GasPrice(0), 2, GasPrice(0))]
// margin=10ppt (1%). fee_actual=10000. Upper=10000*1010/1000=10100. Lower=10000*1000/1010=9900.
#[case::custom_margin_clamps_to_upper(Some(GasPrice(99999)), GasPrice(10000), 10, GasPrice(10100))]
#[case::custom_margin_clamps_to_lower(Some(GasPrice(1)), GasPrice(10000), 10, GasPrice(9900))]
fn test_compute_fee_proposal(
    #[case] fee_target: Option<GasPrice>,
    #[case] fee_actual: GasPrice,
    #[case] margin_ppt: u128,
    #[case] expected: GasPrice,
) {
    assert_eq!(compute_fee_proposal(fee_target, fee_actual, margin_ppt), expected);
}
