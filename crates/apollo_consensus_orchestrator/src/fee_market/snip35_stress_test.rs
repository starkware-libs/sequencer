//! SNIP-35 stress / correctness / security tests.
//!
//! These tests are NOT intended to be merged. They probe:
//! - correctness of median / clamp / bounds math
//! - completeness (all fallback paths)
//! - security (overflow, extreme inputs, proposer-validator symmetry)
//! - mixed-node scenarios (pre-SNIP-35 nodes producing fee_proposal=0)
//! - sharp price changes (oracle spikes, crashes, recoveries)

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use starknet_api::block::GasPrice;

use crate::snip35::{
    compute_fee_actual,
    compute_fee_proposal,
    compute_fee_target,
    FEE_PROPOSAL_MARGIN_PPT,
    FEE_PROPOSAL_WINDOW_SIZE,
    ORACLE_L2_GAS_FLOOR_MAX_FRI,
    ORACLE_L2_GAS_FLOOR_MIN_FRI,
    PPT_DENOMINATOR,
    TARGET_ATTO_USD_PER_L2_GAS,
};

// ============================================================================
// CORRECTNESS: median math
// ============================================================================

#[test]
fn median_of_u128_max_does_not_overflow() {
    // All u128::MAX values — naive (a+b)/2 would overflow.
    let proposals = vec![GasPrice(u128::MAX); 10];
    assert_eq!(compute_fee_actual(&proposals, 10), Some(GasPrice(u128::MAX)));
}

#[test]
fn median_of_max_and_near_max() {
    // Pairs close to u128::MAX that would overflow naive averaging.
    let proposals = vec![
        GasPrice(u128::MAX - 10),
        GasPrice(u128::MAX - 8),
        GasPrice(u128::MAX - 6),
        GasPrice(u128::MAX - 4),
        GasPrice(u128::MAX - 2),
        GasPrice(u128::MAX - 1),
        GasPrice(u128::MAX - 3),
        GasPrice(u128::MAX - 5),
        GasPrice(u128::MAX - 7),
        GasPrice(u128::MAX - 9),
    ];
    // Sorted: MAX-10..=MAX-1. Middle two: MAX-6 and MAX-5. Median = MAX-6 + ((MAX-5)-(MAX-6))/2 =
    // MAX-6.
    let expected = GasPrice(u128::MAX - 6);
    assert_eq!(compute_fee_actual(&proposals, 10), Some(expected));
}

#[test]
fn median_is_deterministic_regardless_of_input_order() {
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let base: Vec<GasPrice> = (0..10).map(|_| GasPrice(rng.gen_range(1..1_000_000))).collect();

    let ref_median = compute_fee_actual(&base, 10);

    // Try 100 random permutations — median must be identical.
    for seed in 0..100 {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let mut shuffled = base.clone();
        // Fisher-Yates.
        for i in (1..shuffled.len()).rev() {
            let j = rng.gen_range(0..=i);
            shuffled.swap(i, j);
        }
        assert_eq!(compute_fee_actual(&shuffled, 10), ref_median, "seed={seed}");
    }
}

#[test]
fn median_ignores_prefix_entries_beyond_window() {
    // 100 entries but window_size=10: only last 10 contribute.
    let mut proposals: Vec<GasPrice> = (0..90).map(|i| GasPrice(i * 1_000_000)).collect();
    proposals.extend(vec![GasPrice(500); 10]);
    assert_eq!(compute_fee_actual(&proposals, 10), Some(GasPrice(500)));
}

#[test]
fn median_window_size_exactly_matches() {
    let proposals: Vec<GasPrice> = (1..=10).map(GasPrice).collect();
    // Sorted: 1..=10. Middle: 5,6. Median = 5 + (6-5)/2 = 5.
    assert_eq!(compute_fee_actual(&proposals, 10), Some(GasPrice(5)));
}

// ============================================================================
// CORRECTNESS: fee_target math
// ============================================================================

#[test]
fn fee_target_matches_hand_computed_values() {
    // Canonical reference values at $3e-9/gas.
    // STRK at $1.00: floor = 3e9 * 1e18 / 1e18 = 3_000_000_000 FRI
    assert_eq!(
        compute_fee_target(3_000_000_000, 10u128.pow(18), 0, u128::MAX),
        GasPrice(3_000_000_000)
    );
    // STRK at $0.10: floor = 3e9 * 1e18 / 1e17 = 30_000_000_000 FRI
    assert_eq!(
        compute_fee_target(3_000_000_000, 10u128.pow(17), 0, u128::MAX),
        GasPrice(30_000_000_000)
    );
    // STRK at $2.00: floor = 3e9 * 1e18 / 2e18 = 1_500_000_000 FRI
    assert_eq!(
        compute_fee_target(3_000_000_000, 2 * 10u128.pow(18), 0, u128::MAX),
        GasPrice(1_500_000_000)
    );
}

#[test]
fn fee_target_monotonic_in_strk_price() {
    // As STRK price rises, fewer FRI needed → fee_target drops.
    let target = 3_000_000_000;
    let mut prev = compute_fee_target(target, 10u128.pow(17), 0, u128::MAX); // $0.10
    for exp in 17..=21 {
        let curr = compute_fee_target(target, 10u128.pow(exp), 0, u128::MAX);
        assert!(
            curr.0 <= prev.0,
            "fee_target should be non-increasing in strk_price: prev={} curr={}",
            prev.0,
            curr.0
        );
        prev = curr;
    }
}

#[test]
fn fee_target_clamps_rigorously() {
    // Min clamp: STRK price extremely high → target tiny → clamped to min.
    let t = compute_fee_target(1, u128::MAX, 100_000, u128::MAX);
    assert_eq!(t, GasPrice(100_000));

    // Max clamp: STRK price extremely low → target huge → clamped to max.
    let t = compute_fee_target(u128::MAX, 1, 0, 12345);
    assert_eq!(t, GasPrice(12345));

    // Zero rate: returns max (oracle unreliable fallback).
    assert_eq!(compute_fee_target(100, 0, 0, 999), GasPrice(999));
}

#[test]
fn fee_target_extreme_target_values_do_not_panic() {
    // target_atto_usd * 1e18 can be up to u128::MAX * 1e18 → overflow in naive u128.
    // compute_fee_target uses U256, so should not panic.
    let t = compute_fee_target(u128::MAX, u128::MAX, 0, u128::MAX);
    // Result should saturate but not panic.
    let _ = t.0;

    let t = compute_fee_target(
        u128::MAX / 2,
        1,
        ORACLE_L2_GAS_FLOOR_MIN_FRI,
        ORACLE_L2_GAS_FLOOR_MAX_FRI,
    );
    let _ = t.0;
}

// ============================================================================
// CORRECTNESS: fee_proposal clamping
// ============================================================================

#[test]
fn fee_proposal_bounds_are_exact() {
    // margin=2ppt: upper = actual * 1002/1000, lower = actual * 1000/1002.
    let actual = GasPrice(1_000_000);
    let upper = compute_fee_proposal(Some(GasPrice(u128::MAX)), actual, 2);
    let lower = compute_fee_proposal(Some(GasPrice(0)), actual, 2);
    assert_eq!(upper, GasPrice(1_002_000));
    assert_eq!(lower, GasPrice(998_003)); // 1_000_000 * 1000 / 1002 = 998_003 (floor)
}

#[test]
fn fee_proposal_within_bounds_returns_target() {
    let actual = GasPrice(10_000);
    // Target well within [998_00 / 10, 1_002_00 / 10] = [9980, 10020]
    for target in [9981, 9990, 10_000, 10_010, 10_019].iter() {
        let proposal = compute_fee_proposal(Some(GasPrice(*target)), actual, 2);
        assert_eq!(proposal, GasPrice(*target));
    }
}

#[test]
fn fee_proposal_oracle_failure_freezes_at_actual() {
    for actual in [GasPrice(1), GasPrice(1_000_000), GasPrice(u128::MAX - 1)] {
        assert_eq!(compute_fee_proposal(None, actual, 2), actual);
    }
}

#[test]
fn fee_proposal_saturating_on_extreme_actual() {
    // actual near u128::MAX — saturating_mul prevents overflow.
    let actual = GasPrice(u128::MAX);
    let prop = compute_fee_proposal(Some(GasPrice(1)), actual, 2);
    // Should not panic; returns some clamped value.
    let _ = prop.0;
}

#[test]
fn fee_proposal_zero_actual_always_returns_zero_with_oracle() {
    for target in [0, 1, 1_000_000, u128::MAX].iter() {
        let p = compute_fee_proposal(Some(GasPrice(*target)), GasPrice(0), 2);
        assert_eq!(p, GasPrice(0));
    }
}

// ============================================================================
// PROPOSER-VALIDATOR SYMMETRY (consensus correctness)
// ============================================================================

/// Validator's accept predicate MUST match what honest proposers produce.
/// This is the core consensus safety property.
fn validator_accepts(fee_actual: GasPrice, fee_proposal: GasPrice, margin_ppt: u128) -> bool {
    let lower = fee_actual.0.saturating_mul(PPT_DENOMINATOR) / (PPT_DENOMINATOR + margin_ppt);
    let upper = fee_actual.0.saturating_mul(PPT_DENOMINATOR + margin_ppt) / PPT_DENOMINATOR;
    fee_proposal.0 >= lower && fee_proposal.0 <= upper
}

#[test]
fn honest_proposer_always_passes_validation() {
    let margin = FEE_PROPOSAL_MARGIN_PPT;
    let fee_actuals = [1u128, 100, 10_000, 8_000_000_000, 1_000_000_000_000_000_000];
    let strk_rates = [
        10u128.pow(15),
        10u128.pow(16),
        10u128.pow(17),
        10u128.pow(18),
        10u128.pow(19),
        5 * 10u128.pow(17),
        3 * 10u128.pow(18),
    ];

    for &fa in &fee_actuals {
        let fee_actual = GasPrice(fa);
        // Oracle failure path:
        let prop_no_oracle = compute_fee_proposal(None, fee_actual, margin);
        assert!(
            validator_accepts(fee_actual, prop_no_oracle, margin),
            "oracle failure: fa={fa} prop={}",
            prop_no_oracle.0
        );

        // Oracle success path:
        for &rate in &strk_rates {
            let target = compute_fee_target(
                TARGET_ATTO_USD_PER_L2_GAS,
                rate,
                ORACLE_L2_GAS_FLOOR_MIN_FRI,
                ORACLE_L2_GAS_FLOOR_MAX_FRI,
            );
            let prop = compute_fee_proposal(Some(target), fee_actual, margin);
            assert!(
                validator_accepts(fee_actual, prop, margin),
                "fa={fa} rate={rate} prop={} target={}",
                prop.0,
                target.0
            );
        }
    }
}

#[test]
fn honest_proposer_always_passes_validation_fuzzed() {
    let mut rng = ChaCha8Rng::seed_from_u64(0xDEADBEEF);
    let margin = FEE_PROPOSAL_MARGIN_PPT;

    for _ in 0..10_000 {
        let fa = rng.gen_range(1u128..1_000_000_000_000_000_000);
        let rate = rng.gen_range(1u128..2 * 10u128.pow(18));
        let fee_actual = GasPrice(fa);
        let target = compute_fee_target(
            TARGET_ATTO_USD_PER_L2_GAS,
            rate,
            ORACLE_L2_GAS_FLOOR_MIN_FRI,
            ORACLE_L2_GAS_FLOOR_MAX_FRI,
        );
        let oracle_result = if rng.gen_bool(0.1) { None } else { Some(target) };
        let prop = compute_fee_proposal(oracle_result, fee_actual, margin);
        assert!(
            validator_accepts(fee_actual, prop, margin),
            "fa={fa} rate={rate} oracle={oracle_result:?} prop={}",
            prop.0
        );
    }
}

// ============================================================================
// SECURITY: adversarial proposer
// ============================================================================

#[test]
fn malicious_high_proposal_rejected() {
    let fee_actual = GasPrice(1_000_000);
    // Upper bound = 1_002_000. Anything above must be rejected.
    for proposal in [1_002_001u128, 1_003_000, 2_000_000, u128::MAX].iter() {
        assert!(
            !validator_accepts(fee_actual, GasPrice(*proposal), 2),
            "malicious high {proposal} was accepted"
        );
    }
}

#[test]
fn malicious_low_proposal_rejected() {
    let fee_actual = GasPrice(1_000_000);
    // Lower bound = 998_003. Anything below must be rejected.
    for proposal in [0u128, 1, 998_002, 500_000].iter() {
        assert!(
            !validator_accepts(fee_actual, GasPrice(*proposal), 2),
            "malicious low {proposal} was accepted"
        );
    }
}

#[test]
fn proposer_cannot_escape_margin_via_oracle_lies() {
    // Even if the oracle returns a wildly wrong value, the proposer's honest clamp
    // must produce a value the validator will accept. This verifies the clamp is tight.
    let fee_actual = GasPrice(1_000_000_000);
    let upper = fee_actual.0.saturating_mul(1002) / 1000; // 1_002_000_000
    let lower = fee_actual.0.saturating_mul(1000) / 1002; // 998_003_992

    // Oracle claims stratospheric price → proposer clamps to upper.
    let prop = compute_fee_proposal(Some(GasPrice(u128::MAX)), fee_actual, 2);
    assert_eq!(prop, GasPrice(upper));
    assert!(validator_accepts(fee_actual, prop, 2));

    // Oracle claims near-zero price → proposer clamps to lower.
    let prop = compute_fee_proposal(Some(GasPrice(0)), fee_actual, 2);
    assert_eq!(prop, GasPrice(lower));
    assert!(validator_accepts(fee_actual, prop, 2));
}

// ============================================================================
// SHARP PRICE CHANGES (the requested scenario)
// ============================================================================

/// Simulate a chain over N blocks with a price shock.
/// Returns the sequence of fee_proposals published by an honest proposer.
fn simulate_chain(
    initial_window: Vec<GasPrice>,
    strk_rates_per_block: &[u128],
    fallback_when_window_short: GasPrice,
) -> Vec<GasPrice> {
    let mut window: std::collections::VecDeque<GasPrice> = initial_window.into();
    let mut proposals = Vec::with_capacity(strk_rates_per_block.len());

    for &rate in strk_rates_per_block {
        let window_vec: Vec<GasPrice> = window.iter().copied().collect();
        let fee_actual = compute_fee_actual(&window_vec, FEE_PROPOSAL_WINDOW_SIZE)
            .unwrap_or(fallback_when_window_short);
        let target = if rate == 0 {
            None
        } else {
            Some(compute_fee_target(
                TARGET_ATTO_USD_PER_L2_GAS,
                rate,
                ORACLE_L2_GAS_FLOOR_MIN_FRI,
                ORACLE_L2_GAS_FLOOR_MAX_FRI,
            ))
        };
        let prop = compute_fee_proposal(target, fee_actual, FEE_PROPOSAL_MARGIN_PPT);
        proposals.push(prop);
        if window.len() >= FEE_PROPOSAL_WINDOW_SIZE {
            window.pop_front();
        }
        window.push_back(prop);
    }
    proposals
}

#[test]
fn strk_price_doubles_overnight_fee_decreases_gradually() {
    // Start with stable window at 12B (above min floor 8B).
    // STRK rate gives target=8B (= min floor). Since 8B < fee_actual=12B, proposer
    // outputs lower bound every block and fee drops ~0.2%/block.
    let fallback = GasPrice(12_000_000_000);
    let initial = vec![GasPrice(12_000_000_000); 10];

    // Rate high enough that target clamps to min floor.
    let rates = vec![10u128.pow(20); 100];
    let proposals = simulate_chain(initial, &rates, fallback);

    // Bounds on per-block drift.
    for w in proposals.windows(2) {
        let prev = w[0].0;
        let curr = w[1].0;
        let max_delta = prev * 2 / 1000 + 1;
        assert!(prev.abs_diff(curr) <= max_delta, "jumped too fast: {prev} -> {curr}");
    }

    // Fees should have fallen from 12B but not crashed instantly.
    let last = proposals.last().unwrap().0;
    assert!(last < 12_000_000_000, "did not drop: {last}");
    // Min floor is 8B, so can't drop below that regardless.
    assert!(last >= 8_000_000_000, "crashed below min floor: {last}");
}

#[test]
fn strk_price_crashes_90_percent_fee_rises_gradually() {
    // STRK crashes from $1.00 to $0.10. Fee must rise gradually (~0.2%/block).
    let fallback = GasPrice(8_000_000_000);
    let initial = vec![GasPrice(3_000_000_000); 10]; // $1.00 steady state

    let rates = vec![10u128.pow(17); 200]; // $0.10
    let proposals = simulate_chain(initial, &rates, fallback);

    for w in proposals.windows(2) {
        let prev = w[0].0;
        let curr = w[1].0;
        let max_delta = prev * 2 / 1000 + 1;
        assert!(prev.abs_diff(curr) <= max_delta);
    }

    // Over 200 blocks at 0.2%/block, price can rise by ~e^0.4 ≈ 1.49x.
    let final_price = proposals.last().unwrap().0;
    assert!(final_price > 3_000_000_000);
    assert!(final_price < 30_000_000_000, "rose too fast");
}

#[test]
fn oracle_flaps_on_off_bounded_drift() {
    // Oracle alternates between valid rate and failure. Each block, fee_proposal
    // moves at most 0.2% relative to the current fee_actual (not necessarily the
    // previous proposal). So check drift against fee_actual (= median), not prev.
    let fallback = GasPrice(12_000_000_000);
    let initial = vec![GasPrice(12_000_000_000); 10];

    let rates: Vec<u128> =
        (0..200).map(|i| if i % 2 == 0 { 5 * 10u128.pow(17) } else { 0 }).collect();
    let proposals = simulate_chain(initial, &rates, fallback);

    // Running median (fee_actual) tracked externally.
    let mut window: std::collections::VecDeque<GasPrice> =
        vec![GasPrice(12_000_000_000); 10].into();
    for p in &proposals {
        let w: Vec<GasPrice> = window.iter().copied().collect();
        let fa = compute_fee_actual(&w, 10).unwrap_or(fallback);
        let lower = fa.0.saturating_mul(1000) / 1002;
        let upper = fa.0.saturating_mul(1002) / 1000;
        assert!(p.0 >= lower && p.0 <= upper, "proposal {} out of [{lower}, {upper}]", p.0);
        window.pop_front();
        window.push_back(*p);
    }
}

#[test]
fn oracle_crashes_for_100_blocks_then_returns() {
    let fallback = GasPrice(8_000_000_000);
    let initial = vec![GasPrice(6_000_000_000); 10];

    // 100 blocks oracle down (freeze), then back.
    let mut rates = vec![0u128; 100];
    rates.extend(vec![5 * 10u128.pow(17); 50]); // $0.50
    let proposals = simulate_chain(initial, &rates, fallback);

    // First 100: should be flat (freeze at fee_actual).
    for i in 1..100 {
        // After initial churn, the window stabilizes.
        if i >= 10 {
            assert_eq!(
                proposals[i - 1].0,
                proposals[i].0,
                "not flat during oracle crash at block {i}"
            );
        }
    }
    // Last one: some drift.
    let _ = proposals[149];
}

#[test]
fn pathological_oracle_oscillation_symmetric() {
    // Oracle returns wildly swinging rates. After many blocks, fee_proposal should
    // settle near equilibrium.
    let fallback = GasPrice(8_000_000_000);
    let initial = vec![GasPrice(6_000_000_000); 10];

    // Oracle oscillates +/- 10x around $0.50.
    let rates: Vec<u128> =
        (0..500).map(|i| if i % 2 == 0 { 10u128.pow(17) } else { 10u128.pow(19) }).collect();
    let proposals = simulate_chain(initial, &rates, fallback);

    // After oscillation, max/min ratio should be moderate (constrained by margin).
    let last_50 = &proposals[450..];
    let max = last_50.iter().map(|p| p.0).max().unwrap();
    let min = last_50.iter().map(|p| p.0).min().unwrap();
    // Over 50 blocks of oscillation with 0.2%/block, ratio bounded by ~1.002^50 ≈ 1.105.
    // max/min <= 1.2 iff max * 10 <= min * 12 (integer comparison avoids lossy f64 cast).
    assert!(
        max.saturating_mul(10) <= min.saturating_mul(12),
        "oscillation amplified: {min}..{max}"
    );
}

// ============================================================================
// MIXED NODES: some have SNIP-35, some don't
// ============================================================================

#[test]
fn pre_snip35_blocks_have_zero_fee_proposal_trigger_fallback() {
    // All-zero window means pre-SNIP-35 history — compute_fee_actual returns None,
    // triggering the l2_gas_price fallback path.
    let window = vec![GasPrice(0); 10];
    assert_eq!(compute_fee_actual(&window, 10), None);
}

#[test]
fn mixed_window_with_one_snip35_block_computes_low_median() {
    // 9 pre-SNIP-35 blocks (0) + 1 new proposal (N) — median is 0, returns None.
    let mut window = vec![GasPrice(0); 9];
    window.push(GasPrice(1_000_000));
    // Sorted: [0,0,0,0,0,0,0,0,0,1_000_000]. Middle: 0,0. Median = 0.
    assert_eq!(compute_fee_actual(&window, 10), None);
}

#[test]
fn mixed_window_with_six_snip35_blocks_kicks_in() {
    // 4 pre-SNIP-35 (0) + 6 new proposals (N) — median finally nonzero.
    let mut window = vec![GasPrice(0); 4];
    window.extend(vec![GasPrice(1_000_000); 6]);
    // Sorted: [0,0,0,0,1M,1M,1M,1M,1M,1M]. Middle: 1M,1M. Median = 1M.
    assert_eq!(compute_fee_actual(&window, 10), Some(GasPrice(1_000_000)));
}

#[test]
fn transition_from_pre_to_post_snip35_converges_to_oracle() {
    // Start with all-zero (pre-SNIP-35). Every block, a new post-SNIP-35 proposal
    // is appended. Eventually fee_actual becomes nonzero.
    let fallback = GasPrice(6_000_000_000);
    let mut window: std::collections::VecDeque<GasPrice> = vec![GasPrice(0); 10].into();
    let mut transition_block: Option<usize> = None;

    for i in 0..30 {
        let window_vec: Vec<GasPrice> = window.iter().copied().collect();
        let fa = compute_fee_actual(&window_vec, 10);
        if fa.is_some() && transition_block.is_none() {
            transition_block = Some(i);
        }
        let actual = fa.unwrap_or(fallback);
        let target = compute_fee_target(
            TARGET_ATTO_USD_PER_L2_GAS,
            5 * 10u128.pow(17), // $0.50
            ORACLE_L2_GAS_FLOOR_MIN_FRI,
            ORACLE_L2_GAS_FLOOR_MAX_FRI,
        );
        let prop = compute_fee_proposal(Some(target), actual, FEE_PROPOSAL_MARGIN_PPT);
        window.pop_front();
        window.push_back(prop);
    }

    // Transition must happen around when post-SNIP-35 blocks dominate the window.
    let block = transition_block.expect("never transitioned");
    assert!((5..=10).contains(&block), "transition at unexpected block: {block}");
}

#[test]
fn validator_does_not_enforce_bounds_during_initiation() {
    // When fee_actual is None (<10 post-SNIP-35 blocks), any fee_proposal passes.
    // This mimics the validate_proposal.rs `if let Some(fee_actual) = ...` guard.
    let fee_actual: Option<GasPrice> = None;
    // Any proposer-chosen fee_proposal should be accepted.
    for p in [0u128, 1, 1_000_000, u128::MAX].iter() {
        let accepted = match fee_actual {
            None => true,
            Some(actual) => validator_accepts(actual, GasPrice(*p), 2),
        };
        assert!(accepted, "initiation rejection");
    }
}

// ============================================================================
// PROPERTY: long-run steady state
// ============================================================================

#[test]
fn steady_oracle_converges_to_fee_target() {
    // With a constant oracle rate for 2000 blocks, fee_proposal should converge
    // close to fee_target.
    let fallback = GasPrice(6_000_000_000);
    let initial = vec![GasPrice(6_000_000_000); 10];

    let rate = 5 * 10u128.pow(17); // $0.50
    let expected_target = compute_fee_target(
        TARGET_ATTO_USD_PER_L2_GAS,
        rate,
        ORACLE_L2_GAS_FLOOR_MIN_FRI,
        ORACLE_L2_GAS_FLOOR_MAX_FRI,
    );
    let rates = vec![rate; 2000];
    let proposals = simulate_chain(initial, &rates, fallback);

    // Last proposal should be within 1% of target.
    let last = proposals.last().unwrap().0;
    let diff = last.abs_diff(expected_target.0);
    assert!(
        diff <= expected_target.0 / 100,
        "did not converge: last={last} target={}",
        expected_target.0
    );
}

#[test]
fn window_size_1_is_safe() {
    // Edge: window_size=1 should return None (we enforce >= 2 for sanity).
    assert_eq!(compute_fee_actual(&[GasPrice(42)], 1), None);
}

#[test]
fn window_size_0_is_safe() {
    assert_eq!(compute_fee_actual(&[], 0), None);
}

#[test]
fn window_size_exactly_2_works() {
    // Median of 2 elements: a + (b-a)/2 rounded down.
    let p = vec![GasPrice(10), GasPrice(20)];
    assert_eq!(compute_fee_actual(&p, 2), Some(GasPrice(15)));

    let p = vec![GasPrice(10), GasPrice(11)];
    assert_eq!(compute_fee_actual(&p, 2), Some(GasPrice(10)));
}

// ============================================================================
// INVARIANT: margin symmetry
// ============================================================================

#[test]
fn margin_bounds_are_approximately_symmetric() {
    // For small margin_ppt, upper/actual and actual/lower should be close.
    let actual = 1_000_000_000u128;
    let upper = actual.saturating_mul(1002) / 1000;
    let lower = actual.saturating_mul(1000) / 1002;

    // upper/actual ≈ actual/lower  ⇔  upper * lower ≈ actual * actual
    // (integer comparison avoids lossy f64 casts of u128). Tolerance 1e-6 on ratios
    // corresponds to |upper*lower - actual^2| < actual^2 / 1_000_000.
    let actual_sq = actual.saturating_mul(actual);
    let cross = upper.saturating_mul(lower);
    let tolerance = actual_sq / 1_000_000;
    assert!(actual_sq.abs_diff(cross) < tolerance);
}

#[test]
fn larger_margin_expands_bounds() {
    let actual = GasPrice(1_000_000);
    let prop_2ppt = compute_fee_proposal(Some(GasPrice(u128::MAX)), actual, 2);
    let prop_10ppt = compute_fee_proposal(Some(GasPrice(u128::MAX)), actual, 10);
    let prop_100ppt = compute_fee_proposal(Some(GasPrice(u128::MAX)), actual, 100);
    assert!(prop_2ppt.0 < prop_10ppt.0);
    assert!(prop_10ppt.0 < prop_100ppt.0);
}

// ============================================================================
// INVARIANT: fee_actual is a valid proposal for itself
// ============================================================================

#[test]
fn fee_actual_passes_its_own_validation() {
    // If oracle fails, proposer uses fee_actual. Validator must accept it.
    for fa in [1u128, 100, 10_000, 1_000_000_000_000].iter() {
        assert!(validator_accepts(GasPrice(*fa), GasPrice(*fa), 2));
    }
}

// ============================================================================
// ADVERSARIAL: can an attacker drift fee_actual over time?
// ============================================================================

#[test]
fn single_proposer_drift_is_bounded_and_visible() {
    // Attacker proposes upper bound every block. Observe how much fee_actual drifts.
    let fallback = GasPrice(6_000_000_000);
    let mut window: std::collections::VecDeque<GasPrice> = vec![fallback; 10].into();
    let start_fa = compute_fee_actual(&window.iter().copied().collect::<Vec<_>>(), 10).unwrap();
    let margin = FEE_PROPOSAL_MARGIN_PPT;

    for _ in 0..1000 {
        let window_vec: Vec<GasPrice> = window.iter().copied().collect();
        let fa = compute_fee_actual(&window_vec, 10).unwrap();
        let upper = fa.0.saturating_mul(PPT_DENOMINATOR + margin) / PPT_DENOMINATOR;
        window.pop_front();
        window.push_back(GasPrice(upper));
    }

    let end_fa = compute_fee_actual(&window.iter().copied().collect::<Vec<_>>(), 10).unwrap();
    // After 1000 blocks, the median lags behind the attacker's upper bounds due to
    // median resistance, but should show meaningful drift.
    // Check 1 < end/start < 10 via integer comparisons (avoids lossy u128 → f64).
    assert!(end_fa.0 > start_fa.0, "attacker made no progress: {} → {}", start_fa.0, end_fa.0);
    // Upper bound on drift: each proposal is +0.2% of current median. Median moves
    // slower than the proposal stream. Over 1000 blocks, drift is significant but
    // bounded.
    assert!(
        end_fa.0 < start_fa.0.saturating_mul(10),
        "unrealistically large drift: {} → {}",
        start_fa.0,
        end_fa.0
    );
}

#[test]
fn lone_malicious_proposer_in_window_cannot_skew_median() {
    // Honest chain at value A. Attacker slips one wildly different proposal into the window.
    // Median should resist.
    let mut window = vec![GasPrice(1_000_000); 9];
    window.push(GasPrice(u128::MAX / 2)); // adversary
    let median = compute_fee_actual(&window, 10).unwrap();
    // Median of 9x 1M and 1 huge value: middle pair is (1M, 1M) → median = 1M.
    assert_eq!(median, GasPrice(1_000_000));
}

#[test]
fn five_malicious_proposers_in_window_can_move_median() {
    // With 5 adversarial blocks in a 10-window, they control one of the middle values.
    let mut window = vec![GasPrice(1_000_000); 5]; // honest
    window.extend(vec![GasPrice(2_000_000); 5]); // adversarial
    let median = compute_fee_actual(&window, 10).unwrap();
    // Sorted: 5x 1M, 5x 2M. Middle: 1M, 2M. Median = 1M + (2M-1M)/2 = 1.5M.
    assert_eq!(median, GasPrice(1_500_000));
}
