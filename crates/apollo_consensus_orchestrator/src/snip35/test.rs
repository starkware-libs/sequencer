use starknet_api::block::GasPrice;

use crate::snip35::{compute_fee_actual, compute_fee_proposal, compute_fee_target};

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

#[test]
fn test_compute_fee_actual_u128_max_does_not_overflow() {
    // Naive (a+b)/2 would overflow when a and b are near u128::MAX.
    let proposals = vec![GasPrice(u128::MAX); 10];
    assert_eq!(compute_fee_actual(&proposals, 10), Some(GasPrice(u128::MAX)));
}

#[test]
fn test_compute_fee_target_extreme_values_do_not_panic() {
    // The U256 internal arithmetic must saturate, not panic.
    let _ = compute_fee_target(u128::MAX, u128::MAX, 0, u128::MAX);
    let _ = compute_fee_target(u128::MAX, 1, 0, u128::MAX);
    let _ = compute_fee_target(1, u128::MAX, 0, u128::MAX);
}

#[test]
fn test_compute_fee_proposal_saturating_on_extreme_actual() {
    // actual near u128::MAX: saturating_mul must prevent overflow.
    let _ = compute_fee_proposal(Some(GasPrice(1)), GasPrice(u128::MAX), 2);
    let _ = compute_fee_proposal(Some(GasPrice(u128::MAX)), GasPrice(u128::MAX), 2);
}

#[test]
fn test_compute_fee_target_monotonic_in_strk_price() {
    // As STRK/USD rises, fewer FRI needed → fee_target monotonically decreases.
    let target = 3_000_000_000;
    let mut prev = compute_fee_target(target, 10u128.pow(17), 0, u128::MAX);
    for exp in 17..=21 {
        let curr = compute_fee_target(target, 10u128.pow(exp), 0, u128::MAX);
        assert!(curr.0 <= prev.0, "not monotonic: prev={} curr={}", prev.0, curr.0);
        prev = curr;
    }
}

#[test]
fn test_compute_fee_actual_lone_adversary_cannot_skew_median() {
    // With 9 honest values and 1 outlier, median resists the adversary.
    let mut window = vec![GasPrice(1_000_000); 9];
    window.push(GasPrice(u128::MAX / 2));
    assert_eq!(compute_fee_actual(&window, 10), Some(GasPrice(1_000_000)));
}
