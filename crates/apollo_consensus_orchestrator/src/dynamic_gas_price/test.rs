use std::collections::BTreeMap;

use apollo_consensus_orchestrator_config::config::DEFAULT_SNIP35_TARGET_ATTO_USD_PER_L2_GAS;
use apollo_versioned_constants::VersionedConstants;
use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha8Rng;
use rstest::rstest;
use starknet_api::block::{BlockNumber, GasPrice};
use starknet_api::versioned_constants_logic::VersionedConstantsTrait;

use crate::dynamic_gas_price::{
    compute_fee_actual,
    compute_fee_proposal,
    compute_fee_target,
    fee_proposal_bounds,
    FeeProposalInfo,
    PPT_DENOMINATOR,
};
use crate::test_utils::{INERT_MAX_L2_GAS_PRICE, TEST_MAX_L2_GAS_PRICE, TEST_MIN_L2_GAS_PRICE};

const TEST_FEE_PROPOSAL_WINDOW_SIZE: u64 = 10;

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

fn window_from(
    entries: impl IntoIterator<Item = (u64, Option<GasPrice>)>,
) -> BTreeMap<BlockNumber, Option<GasPrice>> {
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
    // - Heights 0..9 instead of 2..11: sorted = [1, 42, 87, 100, 271, 314, 1024, 1729, 6000, 9999],
    //   median = 271 + (314 - 271) / 2 = 292.
    // - All 12 (window-size ignored): sorted[5..7] = [271, 314], median = 292.
    let values: [u128; 12] = [314, 1729, 42, 1024, 100, 9999, 87, 271, 1, 6000, 17, 999];
    let window = window_from((0u64..).zip(values).map(|(h, v)| (h, Some(GasPrice(v)))));
    assert_eq!(
        compute_fee_actual(&window, BlockNumber(12), TEST_FEE_PROPOSAL_WINDOW_SIZE),
        Some(GasPrice(185))
    );
}

#[rstest]
// Heights [0, 9] needed for fee_actual at height 10; height 5 is missing.
#[case::missing_entry(
    window_from((0u64..10).filter(|h| *h != 5).map(|h| (h, Some(GasPrice(100))))),
    BlockNumber(10),
)]
// Heights [0, 9] needed; height 7 is recorded as None (pre-V0_14_3 block).
#[case::none_entry(
    window_from((0u64..10).map(|h| (h, (h != 7).then_some(GasPrice(100))))),
    BlockNumber(10),
)]
// `current_height < window_size`: the range cannot cover 10 prior heights.
#[case::height_below_window_size(
    window_from((0u64..5).map(|h| (h, Some(GasPrice(100))))),
    BlockNumber(5),
)]
fn test_compute_fee_actual_returns_none(
    #[case] window: BTreeMap<BlockNumber, Option<GasPrice>>,
    #[case] height: BlockNumber,
) {
    assert_eq!(compute_fee_actual(&window, height, TEST_FEE_PROPOSAL_WINDOW_SIZE), None);
}

#[rstest]
// Target: $3e-9/gas = 3_000_000_000 atto-USD. STRK at $0.50 = 500_000_000_000_000_000.
// floor = 3_000_000_000 * 10^18 / 500_000_000_000_000_000 = 6_000_000_000.
#[case::normal(3_000_000_000, 500_000_000_000_000_000, Some(GasPrice(6_000_000_000)))]
// floor = 1 * 10^18 / 10^18 = 1.
#[case::low_target(1, FRI_DECIMALS_SCALE, Some(GasPrice(1)))]
// Extreme target with rate=1: numerator = u128::MAX * 10^18 overflows u128;
// `u128::try_from` saturates the result to u128::MAX.
#[case::saturates_at_u128_max(u128::MAX, 1, Some(GasPrice(u128::MAX)))]
// rate=0 is treated as oracle failure: callers freeze at fee_actual.
#[case::zero_rate_returns_none(3_000_000_000, 0, None)]
fn test_compute_fee_target(
    #[case] target_atto_usd_per_l2_gas: u128,
    #[case] strk_usd_rate: u128,
    #[case] expected: Option<GasPrice>,
) {
    let actual = compute_fee_target(target_atto_usd_per_l2_gas, strk_usd_rate);
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
    assert_eq!(
        compute_fee_proposal(fee_target, fee_actual, margin_ppt, INERT_MAX_L2_GAS_PRICE),
        expected
    );
}

#[test]
fn test_compute_fee_actual_u128_max_does_not_overflow() {
    // Naive (a+b)/2 would overflow when a and b are near u128::MAX.
    let window = window_from((0u64..10).map(|h| (h, Some(GasPrice(u128::MAX)))));
    assert_eq!(
        compute_fee_actual(&window, BlockNumber(10), TEST_FEE_PROPOSAL_WINDOW_SIZE),
        Some(GasPrice(u128::MAX))
    );
}

#[test]
fn test_compute_fee_target_extreme_values_do_not_panic() {
    // The U256 internal arithmetic must saturate, not panic.
    let _ = compute_fee_target(u128::MAX, u128::MAX);
    let _ = compute_fee_target(u128::MAX, 1);
    let _ = compute_fee_target(1, u128::MAX);
}

#[test]
fn test_compute_fee_proposal_saturating_on_extreme_actual() {
    // actual near u128::MAX: saturating_mul must prevent overflow.
    let _ = compute_fee_proposal(Some(GasPrice(1)), GasPrice(u128::MAX), 2, INERT_MAX_L2_GAS_PRICE);
    let _ = compute_fee_proposal(
        Some(GasPrice(u128::MAX)),
        GasPrice(u128::MAX),
        2,
        INERT_MAX_L2_GAS_PRICE,
    );
    // A ceiling of 1 against the widest possible band: both edges collapse onto it.
    assert_eq!(
        compute_fee_proposal(Some(GasPrice(u128::MAX)), GasPrice(u128::MAX), 2, GasPrice(1)),
        GasPrice(1)
    );
}

#[test]
fn test_compute_fee_target_monotonic_in_strk_price() {
    // As STRK/USD rises, fewer FRI needed → fee_target monotonically decreases.
    let target = 3_000_000_000;
    let mut prev = compute_fee_target(target, 10u128.pow(17)).unwrap();
    for exp in 17..=21 {
        let curr = compute_fee_target(target, 10u128.pow(exp)).unwrap();
        assert!(curr.0 <= prev.0, "not monotonic: prev={} curr={}", prev.0, curr.0);
        prev = curr;
    }
}

#[test]
fn test_compute_fee_actual_lone_adversary_cannot_skew_median() {
    // With 9 honest values and 1 outlier, median resists the adversary.
    let mut values = vec![GasPrice(1_000_000); 9];
    values.push(GasPrice(u128::MAX / 2));
    let window = window_from((0u64..).zip(values).map(|(h, v)| (h, Some(v))));
    assert_eq!(
        compute_fee_actual(&window, BlockNumber(10), TEST_FEE_PROPOSAL_WINDOW_SIZE),
        Some(GasPrice(1_000_000))
    );
}

/// The validator's accept predicate. Must stay in sync with
/// `validate_proposal::is_proposal_init_valid` fee_proposal bounds check.
///
/// Recomputes the band instead of calling `fee_proposal_bounds`, so that a proposal produced by
/// `compute_fee_proposal` is checked against an independent formula rather than against the very
/// bounds it was clamped into.
fn validator_accepts(
    fee_actual: GasPrice,
    fee_proposal: GasPrice,
    margin_ppt: u128,
    max_l2_gas_price: GasPrice,
) -> bool {
    let lower = fee_actual.0.saturating_mul(PPT_DENOMINATOR) / (PPT_DENOMINATOR + margin_ppt);
    let upper = fee_actual.0.saturating_mul(PPT_DENOMINATOR + margin_ppt) / PPT_DENOMINATOR;
    let (lower, upper) = (lower.min(max_l2_gas_price.0), upper.min(max_l2_gas_price.0));
    fee_proposal.0 >= lower && fee_proposal.0 <= upper
}

#[test]
fn test_malicious_high_fee_proposal_rejected() {
    // Upper bound for fee_actual=1_000_000 with margin=2ppt is 1_002_000.
    let fee_actual = GasPrice(1_000_000);
    assert!(validator_accepts(fee_actual, GasPrice(1_002_000), 2, INERT_MAX_L2_GAS_PRICE));
    for proposal in [1_002_001u128, 1_003_000, 2_000_000, u128::MAX] {
        assert!(
            !validator_accepts(fee_actual, GasPrice(proposal), 2, INERT_MAX_L2_GAS_PRICE),
            "accepted {proposal}"
        );
    }
}

#[test]
fn test_malicious_low_fee_proposal_rejected() {
    // Lower bound for fee_actual=1_000_000 with margin=2ppt is 998_003.
    let fee_actual = GasPrice(1_000_000);
    assert!(validator_accepts(fee_actual, GasPrice(998_003), 2, INERT_MAX_L2_GAS_PRICE));
    for proposal in [998_002u128, 500_000, 1, 0] {
        assert!(
            !validator_accepts(fee_actual, GasPrice(proposal), 2, INERT_MAX_L2_GAS_PRICE),
            "accepted {proposal}"
        );
    }
}

#[rstest]
// Well below the cap: the plain margin band.
#[case::band_untouched_below_cap(GasPrice(1_000_000), (998_003, 1_002_000))]
// Just below the cap, so only the upper edge is pulled in.
#[case::upper_edge_capped(
    GasPrice(TEST_MAX_L2_GAS_PRICE.0 - 1_000_000),
    (79_839_321_357, TEST_MAX_L2_GAS_PRICE.0)
)]
// Exactly at the cap: `upper` is capped, `lower` is still below it.
#[case::at_cap(TEST_MAX_L2_GAS_PRICE, (79_840_319_361, TEST_MAX_L2_GAS_PRICE.0))]
// Ten times the cap: both edges collapse to a point.
#[case::band_collapses_above_cap(
    GasPrice(TEST_MAX_L2_GAS_PRICE.0 * 10),
    (TEST_MAX_L2_GAS_PRICE.0, TEST_MAX_L2_GAS_PRICE.0)
)]
fn test_fee_proposal_bounds_capped(
    #[case] fee_actual: GasPrice,
    #[case] expected_bounds: (u128, u128),
) {
    assert_eq!(fee_proposal_bounds(fee_actual, 2, TEST_MAX_L2_GAS_PRICE), expected_bounds);
}

#[test]
fn test_honest_proposal_accepted_when_fee_actual_above_cap() {
    // `fee_actual` can exceed the cap when STRK collapses by more than the multiplier.
    let margin_ppt = VersionedConstants::latest_constants().fee_proposal_margin_ppt;
    let fee_actual = GasPrice(TEST_MAX_L2_GAS_PRICE.0 * 10);
    let fee_target = Some(GasPrice(u128::MAX));

    let proposal = compute_fee_proposal(fee_target, fee_actual, margin_ppt, TEST_MAX_L2_GAS_PRICE);

    assert_eq!(proposal, TEST_MAX_L2_GAS_PRICE);
    assert!(validator_accepts(fee_actual, proposal, margin_ppt, TEST_MAX_L2_GAS_PRICE));
}

#[test]
fn test_oracle_failure_above_the_cap_publishes_the_cap() {
    // The frozen `fee_actual` must still be clamped: every validator's band has already
    // collapsed to the single point `max_l2_gas_price`.
    let margin_ppt = VersionedConstants::latest_constants().fee_proposal_margin_ppt;
    let fee_actual = GasPrice(TEST_MAX_L2_GAS_PRICE.0 * 25);

    let proposal = compute_fee_proposal(None, fee_actual, margin_ppt, TEST_MAX_L2_GAS_PRICE);

    assert_eq!(proposal, TEST_MAX_L2_GAS_PRICE);
    assert!(
        validator_accepts(fee_actual, proposal, margin_ppt, TEST_MAX_L2_GAS_PRICE),
        "frozen fee_actual was published above the ceiling and would halt the chain"
    );
}

#[test]
fn test_fee_actual_converges_to_cap_within_one_window() {
    // Once the window holds only capped proposals, their median is at most the cap too.
    let margin_ppt = VersionedConstants::latest_constants().fee_proposal_margin_ppt;
    let mut window = window_from(
        (0..TEST_FEE_PROPOSAL_WINDOW_SIZE)
            .map(|height| (height, Some(GasPrice(TEST_MAX_L2_GAS_PRICE.0.saturating_mul(10))))),
    );

    for height in TEST_FEE_PROPOSAL_WINDOW_SIZE..2 * TEST_FEE_PROPOSAL_WINDOW_SIZE {
        let fee_actual =
            compute_fee_actual(&window, BlockNumber(height), TEST_FEE_PROPOSAL_WINDOW_SIZE)
                .expect("window is fully populated");
        let proposal = compute_fee_proposal(
            Some(GasPrice(u128::MAX)),
            fee_actual,
            margin_ppt,
            TEST_MAX_L2_GAS_PRICE,
        );
        assert_eq!(
            proposal, TEST_MAX_L2_GAS_PRICE,
            "proposal drifted off the cap at height {height}"
        );
        assert!(
            validator_accepts(fee_actual, proposal, margin_ppt, TEST_MAX_L2_GAS_PRICE),
            "honest proposal rejected at height {height}"
        );
        window.insert(BlockNumber(height), Some(proposal));
    }

    let converged_fee_actual = compute_fee_actual(
        &window,
        BlockNumber(2 * TEST_FEE_PROPOSAL_WINDOW_SIZE),
        TEST_FEE_PROPOSAL_WINDOW_SIZE,
    );
    assert_eq!(converged_fee_actual, Some(TEST_MAX_L2_GAS_PRICE));

    // Once converged the band reopens below the cap, so the fee can fall again.
    let (lower, upper) =
        fee_proposal_bounds(TEST_MAX_L2_GAS_PRICE, margin_ppt, TEST_MAX_L2_GAS_PRICE);
    assert!(lower < upper);
    assert_eq!(upper, TEST_MAX_L2_GAS_PRICE.0);
}

#[test]
fn test_malicious_strk_usd_rate_cannot_push_fee_proposal_above_cap() {
    // A rate of 1 atto-USD per STRK drives `compute_fee_target` far above the cap.
    let margin_ppt = VersionedConstants::latest_constants().fee_proposal_margin_ppt;
    let fee_target = compute_fee_target(DEFAULT_SNIP35_TARGET_ATTO_USD_PER_L2_GAS, 1);
    assert!(fee_target.unwrap() > TEST_MAX_L2_GAS_PRICE);

    // Walk the band up from an honest starting point.
    let mut fee_actual = TEST_MIN_L2_GAS_PRICE;
    for _ in 0..10_000 {
        let proposal =
            compute_fee_proposal(fee_target, fee_actual, margin_ppt, TEST_MAX_L2_GAS_PRICE);
        assert!(proposal <= TEST_MAX_L2_GAS_PRICE, "proposal {} exceeded the cap", proposal.0);
        fee_actual = proposal;
    }
    assert_eq!(fee_actual, TEST_MAX_L2_GAS_PRICE);
}

#[test]
fn test_honest_proposer_always_passes_validation_fuzzed() {
    // Consensus safety: whatever compute_fee_proposal produces, the validator accepts.
    // Fuzzes both an out-of-reach ceiling and a binding one, including `fee_actual` above it.
    let margin_ppt = VersionedConstants::latest_constants().fee_proposal_margin_ppt;
    let mut rng = ChaCha8Rng::seed_from_u64(0xDEADBEEF);
    for _ in 0..10_000 {
        let fee_actual_value = rng.random_range(1u128..1_000_000_000_000_000_000);
        let strk_usd_rate = rng.random_range(1u128..2 * 10u128.pow(18));
        let fee_actual = GasPrice(fee_actual_value);
        let target = compute_fee_target(DEFAULT_SNIP35_TARGET_ATTO_USD_PER_L2_GAS, strk_usd_rate);
        let oracle_result = if rng.random_bool(0.1) { None } else { target };
        let max_l2_gas_price = if rng.random_bool(0.5) {
            INERT_MAX_L2_GAS_PRICE
        } else {
            GasPrice(rng.random_range(1u128..2_000_000_000_000_000_000))
        };
        let proposal =
            compute_fee_proposal(oracle_result, fee_actual, margin_ppt, max_l2_gas_price);
        assert!(
            validator_accepts(fee_actual, proposal, margin_ppt, max_l2_gas_price),
            "fee_actual={fee_actual_value} rate={strk_usd_rate} max_l2_gas_price={} proposal={}",
            max_l2_gas_price.0,
            proposal.0
        );
    }
}
