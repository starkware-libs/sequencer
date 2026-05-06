use rstest::rstest;
use starknet_api::block::GasPrice;

use crate::snip35::{
    compute_fee_actual,
    compute_fee_proposal,
    compute_fee_target,
    FeeProposalInfo,
};

#[test]
fn fee_proposal_info_serializes_field_by_name() {
    // The cende blob's wire shape must agree with the Python `FeeProposalInfo` Marshmallow
    // dataclass — same field name (`fee_proposal_fri`), same JSON shape. If a refactor here
    // accidentally renames or restructures the field, the centralized recorder would silently
    // fail to load the Marshmallow schema on the new blob shape.
    let info = FeeProposalInfo { fee_proposal_fri: Some(GasPrice(0x1dcd65000)) };
    let json = serde_json::to_value(&info).unwrap();
    assert_eq!(json["fee_proposal_fri"], serde_json::Value::String("0x1dcd65000".to_string()));

    let info_none = FeeProposalInfo { fee_proposal_fri: None };
    let json_none = serde_json::to_value(&info_none).unwrap();
    assert_eq!(json_none["fee_proposal_fri"], serde_json::Value::Null);
}

#[test]
fn fee_proposal_info_round_trips_through_serde_json() {
    let original = FeeProposalInfo { fee_proposal_fri: Some(GasPrice(0xDEAD_BEEF)) };
    let bytes = serde_json::to_vec(&original).unwrap();
    let reparsed: FeeProposalInfo = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(reparsed, original);

    let original_none = FeeProposalInfo { fee_proposal_fri: None };
    let bytes_none = serde_json::to_vec(&original_none).unwrap();
    let reparsed_none: FeeProposalInfo = serde_json::from_slice(&bytes_none).unwrap();
    assert_eq!(reparsed_none, original_none);
}

const FRI_DECIMALS_SCALE: u128 = 10u128.pow(18);
const FLOOR_MIN_FRI: u128 = 100;
const FLOOR_MAX_FRI: u128 = 1000;

#[test]
fn test_compute_fee_actual_random_window() {
    // 12 values with window_size=10: takes the last 10 (indices 2..11).
    // Excluded values at indices 0..1 are [314, 1729]; including them would shift the median.
    // Sorted window: [1, 17, 42, 87, 100, 271, 999, 1024, 6000, 9999].
    // Even-length median = sorted[4] + (sorted[5] - sorted[4]) / 2 = 100 + 85 = 185.
    //
    // Buggy alternatives (for catching off-by-one or wrong-end-of-window bugs):
    // - First 10 instead of last 10: sorted = [1, 42, 87, 100, 271, 314, 1024, 1729, 6000, 9999],
    //   median = 271 + (314 - 271) / 2 = 292.
    // - All 12 (window_size ignored): sorted[5..7] = [271, 314], median = 292.
    let proposals = vec![
        GasPrice(314),
        GasPrice(1729),
        GasPrice(42),
        GasPrice(1024),
        GasPrice(100),
        GasPrice(9999),
        GasPrice(87),
        GasPrice(271),
        GasPrice(1),
        GasPrice(6000),
        GasPrice(17),
        GasPrice(999),
    ];
    assert_eq!(compute_fee_actual(&proposals, 10), Some(GasPrice(185)));
}

#[test]
fn test_compute_fee_actual_window_size_one_returns_most_recent() {
    let proposals = vec![GasPrice(100), GasPrice(200), GasPrice(300)];
    assert_eq!(compute_fee_actual(&proposals, 1), Some(GasPrice(300)));
}

#[rstest]
#[case::window_size_zero(vec![GasPrice(100); 10], 0)]
#[case::fewer_proposals_than_window(vec![GasPrice(100); 9], 10)]
#[case::all_zero_median(vec![GasPrice(0); 10], 10)]
fn test_compute_fee_actual_returns_none(
    #[case] proposals: Vec<GasPrice>,
    #[case] window_size: usize,
) {
    assert_eq!(compute_fee_actual(&proposals, window_size), None);
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
