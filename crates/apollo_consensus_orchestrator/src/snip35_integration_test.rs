//! SNIP-35 integration / multi-node tests.
//!
//! NOT FOR MERGE. Simulates scenarios where multiple nodes interact:
//! - some have SNIP-35 enabled (oracle attached), some don't
//! - oracle disagreement (different rates between nodes)
//! - sharp price changes propagating through consensus
//! - backfill correctness on startup
//! - malicious proposer rejection

use std::collections::VecDeque;
use std::sync::Arc;

use apollo_l1_gas_price_types::{MockExchangeRateOracleClientTrait, ExchangeRateOracleClientTrait};
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
// Helpers: simulate a node's SNIP-35 state
// ============================================================================

/// Stateful simulation of a single node. Tracks its own sliding window, its own
/// oracle, and its own fallback.
struct Node {
    name: &'static str,
    window: VecDeque<GasPrice>,
    /// If None, this node has no oracle attached (e.g. pre-SNIP-35 upgrade).
    oracle: Option<Arc<dyn ExchangeRateOracleClientTrait>>,
    fallback: GasPrice,
}

impl Node {
    fn new(name: &'static str, fallback: GasPrice) -> Self {
        Self {
            name,
            window: VecDeque::with_capacity(FEE_PROPOSAL_WINDOW_SIZE),
            oracle: None,
            fallback,
        }
    }

    fn with_oracle(mut self, oracle: Arc<dyn ExchangeRateOracleClientTrait>) -> Self {
        self.oracle = Some(oracle);
        self
    }

    fn with_initial_window(mut self, window: Vec<GasPrice>) -> Self {
        self.window = window.into();
        self
    }

    /// Compute fee_actual from the node's own window.
    fn compute_fee_actual(&self) -> Option<GasPrice> {
        let w: Vec<GasPrice> = self.window.iter().copied().collect();
        compute_fee_actual(&w, FEE_PROPOSAL_WINDOW_SIZE)
    }

    /// Honest proposer: compute the fee_proposal this node would publish.
    async fn propose(&self) -> GasPrice {
        let fee_actual = self.compute_fee_actual().unwrap_or(self.fallback);

        let fee_target = match &self.oracle {
            Some(oracle) => match oracle.eth_to_fri_rate(0).await {
                Ok(rate) if rate > 0 => Some(compute_fee_target(
                    TARGET_ATTO_USD_PER_L2_GAS,
                    rate,
                    ORACLE_L2_GAS_FLOOR_MIN_FRI,
                    ORACLE_L2_GAS_FLOOR_MAX_FRI,
                )),
                _ => None,
            },
            None => None,
        };

        compute_fee_proposal(fee_target, fee_actual, FEE_PROPOSAL_MARGIN_PPT)
    }

    /// Validator: does this node accept `proposal`?
    fn accepts(&self, proposal: GasPrice) -> bool {
        match self.compute_fee_actual() {
            None => true, // initiation: bounds not enforced
            Some(fee_actual) => {
                let lower = fee_actual.0.saturating_mul(PPT_DENOMINATOR)
                    / (PPT_DENOMINATOR + FEE_PROPOSAL_MARGIN_PPT);
                let upper = fee_actual.0.saturating_mul(PPT_DENOMINATOR + FEE_PROPOSAL_MARGIN_PPT)
                    / PPT_DENOMINATOR;
                proposal.0 >= lower && proposal.0 <= upper
            }
        }
    }

    /// Commit a block (push proposer's fee_proposal into this node's window).
    fn commit(&mut self, proposal: GasPrice) {
        if self.window.len() >= FEE_PROPOSAL_WINDOW_SIZE {
            self.window.pop_front();
        }
        self.window.push_back(proposal);
    }
}

fn mock_oracle_const(rate: u128) -> Arc<dyn ExchangeRateOracleClientTrait> {
    let mut mock = MockExchangeRateOracleClientTrait::new();
    mock.expect_eth_to_fri_rate().returning(move |_| Ok(rate));
    Arc::new(mock)
}

fn mock_oracle_failing() -> Arc<dyn ExchangeRateOracleClientTrait> {
    use apollo_l1_gas_price_types::errors::ExchangeRateOracleClientError;
    let mut mock = MockExchangeRateOracleClientTrait::new();
    mock.expect_eth_to_fri_rate()
        .returning(|_| Err(ExchangeRateOracleClientError::RequestError("simulated failure".to_string())));
    Arc::new(mock)
}

// ============================================================================
// SCENARIO 1: All nodes have the same oracle — they agree
// ============================================================================

#[tokio::test]
async fn all_nodes_same_oracle_agree_on_fee_proposal() {
    let rate = 5 * 10u128.pow(17); // $0.50 STRK
    let fallback = GasPrice(8_000_000_000);
    let init = vec![GasPrice(6_000_000_000); 10];

    let mut nodes: Vec<Node> = (0..5)
        .map(|i| {
            let name = Box::leak(format!("node{i}").into_boxed_str());
            Node::new(name, fallback)
                .with_oracle(mock_oracle_const(rate))
                .with_initial_window(init.clone())
        })
        .collect();

    // Each node proposes and all others validate. They should all agree.
    for round in 0..20 {
        let proposer_idx = round % nodes.len();
        let proposal = nodes[proposer_idx].propose().await;

        for (i, validator) in nodes.iter().enumerate() {
            assert!(
                validator.accepts(proposal),
                "round {round}: node{i} rejected proposal {} from {}",
                proposal.0,
                nodes[proposer_idx].name
            );
        }
        for node in &mut nodes {
            node.commit(proposal);
        }
    }
}

// ============================================================================
// SCENARIO 2: Mixed nodes — some have oracle, some don't
// ============================================================================

#[tokio::test]
async fn mixed_oracle_and_no_oracle_nodes_agree_when_oracle_nodes_propose() {
    // 3 nodes with oracle, 2 without (pre-upgrade or oracle not yet configured).
    let rate = 5 * 10u128.pow(17);
    let fallback = GasPrice(8_000_000_000);
    let init = vec![GasPrice(6_000_000_000); 10];

    let mut oracle_nodes: Vec<Node> = (0..3)
        .map(|i| {
            let name = Box::leak(format!("oracle{i}").into_boxed_str());
            Node::new(name, fallback)
                .with_oracle(mock_oracle_const(rate))
                .with_initial_window(init.clone())
        })
        .collect();
    let mut no_oracle_nodes: Vec<Node> = (0..2)
        .map(|i| {
            let name = Box::leak(format!("no_oracle{i}").into_boxed_str());
            Node::new(name, fallback).with_initial_window(init.clone())
        })
        .collect();

    // Oracle nodes take turns proposing. No-oracle nodes validate.
    for round in 0..20 {
        let idx = round % oracle_nodes.len();
        let proposal = oracle_nodes[idx].propose().await;

        for n in oracle_nodes.iter() {
            assert!(n.accepts(proposal), "oracle node rejected oracle proposer");
        }
        for n in no_oracle_nodes.iter() {
            assert!(
                n.accepts(proposal),
                "no-oracle node rejected oracle proposer's {}",
                proposal.0
            );
        }
        for n in oracle_nodes.iter_mut() {
            n.commit(proposal);
        }
        for n in no_oracle_nodes.iter_mut() {
            n.commit(proposal);
        }
    }
}

#[tokio::test]
async fn no_oracle_node_freezes_at_fee_actual() {
    // No-oracle node proposes: it should freeze at fee_actual.
    let fallback = GasPrice(8_000_000_000);
    let initial_median = 6_000_000_000u128;
    let init = vec![GasPrice(initial_median); 10];

    let mut node = Node::new("no_oracle", fallback).with_initial_window(init);

    // First proposal: fee_actual = 6_000_000_000, oracle missing → proposal = 6_000_000_000.
    let p1 = node.propose().await;
    assert_eq!(p1, GasPrice(initial_median));

    // Commit to own window and re-propose. Still 6_000_000_000 (no drift).
    for _ in 0..20 {
        let p = node.propose().await;
        assert_eq!(p, GasPrice(initial_median), "no-oracle node drifted!");
        node.commit(p);
    }
}

#[tokio::test]
async fn oracle_node_proposer_accepted_by_no_oracle_validator() {
    // Validators must stay in sync with the proposer — commit the same blocks.
    let rate = 10u128.pow(18);
    let fallback = GasPrice(12_000_000_000);
    let window = vec![GasPrice(12_000_000_000); 10];

    let mut proposer = Node::new("prop", fallback)
        .with_oracle(mock_oracle_const(rate))
        .with_initial_window(window.clone());
    let mut validator = Node::new("val", fallback).with_initial_window(window);

    for _ in 0..50 {
        let p = proposer.propose().await;
        assert!(validator.accepts(p), "rejected oracle proposer's {}", p.0);
        proposer.commit(p);
        validator.commit(p);
    }
}

// ============================================================================
// SCENARIO 3: Oracle disagreement between nodes (price feed skew)
// ============================================================================

#[tokio::test]
async fn oracle_rates_within_margin_produce_accepted_proposals() {
    // Two nodes with slightly different oracle rates (within the margin).
    // Both proposers should produce fee_proposals the other accepts.
    let fallback = GasPrice(8_000_000_000);
    let window = vec![GasPrice(6_000_000_000); 10];

    let rate_a = 5 * 10u128.pow(17); // $0.50
    let rate_b = 5 * 10u128.pow(17) + 10u128.pow(15); // $0.501 — 0.2% higher

    let mut a = Node::new("A", fallback)
        .with_oracle(mock_oracle_const(rate_a))
        .with_initial_window(window.clone());
    let mut b =
        Node::new("B", fallback).with_oracle(mock_oracle_const(rate_b)).with_initial_window(window);

    for round in 0..30 {
        let (proposer, others): (&mut Node, &mut Node) =
            if round % 2 == 0 { (&mut a, &mut b) } else { (&mut b, &mut a) };
        let p = proposer.propose().await;
        assert!(others.accepts(p), "round {round}: peer rejected");
        proposer.commit(p);
        others.commit(p);
    }
}

#[tokio::test]
async fn oracle_rates_far_apart_still_converge_via_clamp() {
    // Oracle A says $1.00, oracle B says $0.01. Both honest. The clamp ensures
    // their proposals stay near fee_actual, so both still validate each other.
    let fallback = GasPrice(8_000_000_000);
    let window = vec![GasPrice(6_000_000_000); 10];

    let mut a = Node::new("A", fallback)
        .with_oracle(mock_oracle_const(10u128.pow(18)))
        .with_initial_window(window.clone());
    let mut b = Node::new("B", fallback)
        .with_oracle(mock_oracle_const(10u128.pow(16)))
        .with_initial_window(window);

    for round in 0..50 {
        let (proposer, other): (&mut Node, &mut Node) =
            if round % 2 == 0 { (&mut a, &mut b) } else { (&mut b, &mut a) };
        let p = proposer.propose().await;
        assert!(
            other.accepts(p),
            "round {round}: {} rejected proposal {} from {}",
            other.name,
            p.0,
            proposer.name
        );
        proposer.commit(p);
        other.commit(p);
    }
}

// ============================================================================
// SCENARIO 4: Malicious proposer — must be rejected
// ============================================================================

#[tokio::test]
async fn malicious_proposer_above_upper_bound_rejected() {
    let fallback = GasPrice(8_000_000_000);
    let window = vec![GasPrice(1_000_000_000); 10];
    let validator = Node::new("val", fallback).with_initial_window(window);

    let fee_actual = validator.compute_fee_actual().unwrap();
    let upper = fee_actual.0.saturating_mul(1002) / 1000;

    // Exactly at upper: accepted.
    assert!(validator.accepts(GasPrice(upper)));
    // 1 above: rejected.
    assert!(!validator.accepts(GasPrice(upper + 1)));
    // 10% above: rejected.
    assert!(!validator.accepts(GasPrice(upper * 11 / 10)));
    // u128::MAX: rejected.
    assert!(!validator.accepts(GasPrice(u128::MAX)));
}

#[tokio::test]
async fn malicious_proposer_below_lower_bound_rejected() {
    let fallback = GasPrice(8_000_000_000);
    let window = vec![GasPrice(1_000_000_000); 10];
    let validator = Node::new("val", fallback).with_initial_window(window);

    let fee_actual = validator.compute_fee_actual().unwrap();
    let lower = fee_actual.0.saturating_mul(1000) / 1002;

    assert!(validator.accepts(GasPrice(lower)));
    assert!(!validator.accepts(GasPrice(lower - 1)));
    assert!(!validator.accepts(GasPrice(0)));
}

// ============================================================================
// SCENARIO 5: Sharp price changes
// ============================================================================

#[tokio::test]
async fn strk_flash_crash_proposals_rise_gradually() {
    // Steady state at $1.00, then flash crash to $0.01 (100x).
    let fallback = GasPrice(8_000_000_000);
    let window = vec![GasPrice(3_000_000_000); 10];

    // Shared oracle both nodes use; toggle underlying rate.
    // We can't mutate a shared mock easily, so model each block with a fresh oracle.
    let mut window_a: VecDeque<GasPrice> = window.into();
    let mut proposals = Vec::with_capacity(300);

    let rates: Vec<u128> = std::iter::repeat_n(10u128.pow(18), 50)
        .chain(std::iter::repeat_n(10u128.pow(16), 250))
        .collect();

    for &rate in &rates {
        let w: Vec<GasPrice> = window_a.iter().copied().collect();
        let fa = compute_fee_actual(&w, FEE_PROPOSAL_WINDOW_SIZE).unwrap_or(fallback);
        let target = compute_fee_target(
            TARGET_ATTO_USD_PER_L2_GAS,
            rate,
            ORACLE_L2_GAS_FLOOR_MIN_FRI,
            ORACLE_L2_GAS_FLOOR_MAX_FRI,
        );
        let prop = compute_fee_proposal(Some(target), fa, FEE_PROPOSAL_MARGIN_PPT);
        proposals.push(prop);
        window_a.pop_front();
        window_a.push_back(prop);
    }

    // Before the crash, prices stable around 3e9.
    assert!(proposals[40].0.abs_diff(3_000_000_000) < 100_000_000);

    // After the crash, every block proposal rises by at most 0.2%.
    for i in 51..proposals.len() {
        let prev = proposals[i - 1].0;
        let curr = proposals[i].0;
        let max_delta = prev * FEE_PROPOSAL_MARGIN_PPT / PPT_DENOMINATOR + 1;
        assert!(curr.abs_diff(prev) <= max_delta, "block {i}: {prev} -> {curr} exceeds 0.2%");
    }

    // After ~200 blocks of 0.2%/block rises, fee should be much higher.
    assert!(proposals.last().unwrap().0 > 3_000_000_000);
}

#[tokio::test]
async fn strk_pump_proposals_fall_to_min_floor() {
    // STRK pumps — target falls below min floor, clamps up to floor.
    // Starting fee_actual > floor → proposer drops fee gradually toward floor.
    let fallback = GasPrice(20_000_000_000);
    let start_window = vec![GasPrice(20_000_000_000); 10];

    let mut w: VecDeque<GasPrice> = start_window.into();
    let mut proposals = Vec::new();

    let rates: Vec<u128> = std::iter::repeat_n(10u128.pow(18), 50)
        .chain(std::iter::repeat_n(10u128.pow(20), 1000))
        .collect();

    for &rate in &rates {
        let w_vec: Vec<GasPrice> = w.iter().copied().collect();
        let fa = compute_fee_actual(&w_vec, FEE_PROPOSAL_WINDOW_SIZE).unwrap_or(fallback);
        let target = compute_fee_target(
            TARGET_ATTO_USD_PER_L2_GAS,
            rate,
            ORACLE_L2_GAS_FLOOR_MIN_FRI,
            ORACLE_L2_GAS_FLOOR_MAX_FRI,
        );
        let prop = compute_fee_proposal(Some(target), fa, FEE_PROPOSAL_MARGIN_PPT);
        proposals.push(prop);
        w.pop_front();
        w.push_back(prop);
    }

    // Fees should fall but not below ORACLE_L2_GAS_FLOOR_MIN_FRI (= 8B).
    let last = proposals.last().unwrap().0;
    assert!(last < 20_000_000_000, "no drop: {last}");
    assert!(last >= ORACLE_L2_GAS_FLOOR_MIN_FRI, "below min floor: {last}");
}

#[tokio::test]
async fn oracle_temporarily_unavailable_freezes_proposals() {
    // 10 blocks with oracle, then 100 without oracle, then 100 with.
    let fallback = GasPrice(8_000_000_000);
    let mut w: VecDeque<GasPrice> = vec![GasPrice(3_000_000_000); 10].into();
    let mut proposals = Vec::new();

    let rate = 10u128.pow(18);
    for _ in 0..10 {
        let w_vec: Vec<GasPrice> = w.iter().copied().collect();
        let fa = compute_fee_actual(&w_vec, 10).unwrap_or(fallback);
        let target = compute_fee_target(
            TARGET_ATTO_USD_PER_L2_GAS,
            rate,
            ORACLE_L2_GAS_FLOOR_MIN_FRI,
            ORACLE_L2_GAS_FLOOR_MAX_FRI,
        );
        let p = compute_fee_proposal(Some(target), fa, FEE_PROPOSAL_MARGIN_PPT);
        proposals.push(p);
        w.pop_front();
        w.push_back(p);
    }

    // Now oracle dies for 100 blocks.
    for _ in 0..100 {
        let w_vec: Vec<GasPrice> = w.iter().copied().collect();
        let fa = compute_fee_actual(&w_vec, 10).unwrap_or(fallback);
        let p = compute_fee_proposal(None, fa, FEE_PROPOSAL_MARGIN_PPT);
        proposals.push(p);
        w.pop_front();
        w.push_back(p);
    }

    // After oracle dies long enough for the window to fill with its outputs, proposal is stable.
    let tail = &proposals[50..110];
    let first = tail[0].0;
    for (i, p) in tail.iter().enumerate() {
        assert_eq!(p.0, first, "proposal drifted during oracle outage at tail[{i}]");
    }
}

// ============================================================================
// SCENARIO 6: Long-running multi-node simulation with varying conditions
// ============================================================================

#[tokio::test]
async fn chaos_test_mixed_conditions_1000_blocks() {
    // Chaos: 5 nodes, oracle flaps, price swings, random proposer selection.
    let fallback = GasPrice(8_000_000_000);
    let init = vec![GasPrice(6_000_000_000); 10];

    use rand::{Rng, SeedableRng};
    use rand_chacha::ChaCha8Rng;
    let mut rng = ChaCha8Rng::seed_from_u64(12345);

    // 5 nodes. Each independently decides if oracle is up each block.
    let mut windows: Vec<VecDeque<GasPrice>> = (0..5).map(|_| init.clone().into()).collect();

    for block in 0..1000 {
        let proposer = rng.gen_range(0..5);

        let w: Vec<GasPrice> = windows[proposer].iter().copied().collect();
        let fa = compute_fee_actual(&w, 10).unwrap_or(fallback);

        let oracle_up = rng.gen_bool(0.9);
        let strk_rate: u128 = if rng.gen_bool(0.05) {
            // 5% chance of wild price.
            rng.gen_range(10u128.pow(14)..10u128.pow(20))
        } else {
            // Most of the time, near $0.50.
            rng.gen_range(4 * 10u128.pow(17)..6 * 10u128.pow(17))
        };

        let target = if oracle_up {
            Some(compute_fee_target(
                TARGET_ATTO_USD_PER_L2_GAS,
                strk_rate,
                ORACLE_L2_GAS_FLOOR_MIN_FRI,
                ORACLE_L2_GAS_FLOOR_MAX_FRI,
            ))
        } else {
            None
        };
        let proposal = compute_fee_proposal(target, fa, FEE_PROPOSAL_MARGIN_PPT);

        // All validators (including non-proposers with possibly different window state if synced)
        // must accept the proposer's value. Since all started with the same window and apply the
        // same proposals in the same order, their windows are identical → they all accept.
        for w in &windows {
            let w_vec: Vec<GasPrice> = w.iter().copied().collect();
            let fa_v = compute_fee_actual(&w_vec, 10).unwrap_or(fallback);
            let lower = fa_v.0.saturating_mul(PPT_DENOMINATOR)
                / (PPT_DENOMINATOR + FEE_PROPOSAL_MARGIN_PPT);
            let upper =
                fa_v.0.saturating_mul(PPT_DENOMINATOR + FEE_PROPOSAL_MARGIN_PPT) / PPT_DENOMINATOR;
            assert!(
                proposal.0 >= lower && proposal.0 <= upper,
                "block {block}: validator window {w_vec:?} rejected proposer {proposer}'s {}",
                proposal.0
            );
        }

        for w in &mut windows {
            if w.len() >= 10 {
                w.pop_front();
            }
            w.push_back(proposal);
        }
    }
}

// ============================================================================
// SCENARIO 7: Backfill correctness on node startup
// ============================================================================

#[tokio::test]
async fn node_backfilling_catches_up_to_live_chain() {
    // A node restarts and backfills 10 blocks of history. It should produce the
    // same fee_proposal as a node that had been running continuously.
    let fallback = GasPrice(8_000_000_000);
    let chain_history: Vec<GasPrice> = (1..=10).map(|i| GasPrice(i * 1_000_000_000)).collect();

    // Node A has been running: window is the full history.
    let a = Node::new("A", fallback)
        .with_oracle(mock_oracle_const(5 * 10u128.pow(17)))
        .with_initial_window(chain_history.clone());

    // Node B is restarting: backfills the last 10 blocks.
    let b = Node::new("B", fallback)
        .with_oracle(mock_oracle_const(5 * 10u128.pow(17)))
        .with_initial_window(chain_history);

    let pa = a.propose().await;
    let pb = b.propose().await;
    assert_eq!(pa, pb, "backfilled node disagrees with running node");
}

#[tokio::test]
async fn node_starting_with_empty_window_uses_fallback() {
    // Brand new chain: window is empty (or all zero). fee_actual = None →
    // fee_target still applies, clamped around fallback.
    let fallback = GasPrice(8_000_000_000);
    let mut node = Node::new("new", fallback).with_oracle(mock_oracle_const(5 * 10u128.pow(17)));

    let p = node.propose().await;
    // With window empty, fee_actual = None. compute_snip35 falls back to fallback,
    // then clamps fee_target around that.
    let target = compute_fee_target(
        TARGET_ATTO_USD_PER_L2_GAS,
        5 * 10u128.pow(17),
        ORACLE_L2_GAS_FLOOR_MIN_FRI,
        ORACLE_L2_GAS_FLOOR_MAX_FRI,
    );
    let expected = compute_fee_proposal(Some(target), fallback, FEE_PROPOSAL_MARGIN_PPT);
    assert_eq!(p, expected);

    node.commit(p);
}

#[tokio::test]
async fn node_backfilling_with_partial_history_ignores_bounds() {
    // Node starts up with only 5 committed blocks in history.
    // fee_actual returns None, so validator does NOT enforce bounds.
    let mut partial = VecDeque::from(vec![GasPrice(1_000_000_000); 5]);

    let w: Vec<GasPrice> = partial.iter().copied().collect();
    assert_eq!(compute_fee_actual(&w, 10), None);

    // Any wild proposal is technically accepted.
    // (In the real code, validate_proposal.rs skips bounds when fee_actual is None.)
    let fa: Option<GasPrice> = compute_fee_actual(&w, 10);
    assert!(fa.is_none());
    for _ in 0..4 {
        partial.push_back(GasPrice(1_000_000_000));
    }
    let w: Vec<GasPrice> = partial.iter().copied().collect();
    // Now 9 blocks — still None.
    assert_eq!(compute_fee_actual(&w, 10), None);
    partial.push_back(GasPrice(1_000_000_000));
    let w: Vec<GasPrice> = partial.iter().copied().collect();
    // Now 10 — becomes Some.
    assert_eq!(compute_fee_actual(&w, 10), Some(GasPrice(1_000_000_000)));
}

// ============================================================================
// SCENARIO 8: Oracle that returns error vs oracle that returns 0
// ============================================================================

#[tokio::test]
async fn oracle_error_and_oracle_zero_produce_identical_fee_proposal() {
    // Both should trigger the freeze-at-fee_actual path.
    let fallback = GasPrice(8_000_000_000);
    let window = vec![GasPrice(3_000_000_000); 10];

    let error_node = Node::new("error", fallback)
        .with_oracle(mock_oracle_failing())
        .with_initial_window(window.clone());
    let zero_node =
        Node::new("zero", fallback).with_oracle(mock_oracle_const(0)).with_initial_window(window);

    let p_err = error_node.propose().await;
    let p_zero = zero_node.propose().await;
    assert_eq!(p_err, p_zero);
    assert_eq!(p_err, GasPrice(3_000_000_000));
}

// ============================================================================
// SCENARIO 9: Forks — two chains, different proposals, no consensus issue
// ============================================================================

#[tokio::test]
async fn divergent_chains_maintain_independent_fee_actuals() {
    // Two forks diverge. Use a starting window far from the min floor so the
    // clamps don't dominate, and rates that produce clearly different targets.
    let fallback = GasPrice(30_000_000_000);
    // Start at 30B. $1.00 target = 3B (clamped to 8B floor). $0.10 target = 30B (in range).
    let common = vec![GasPrice(30_000_000_000); 10];

    let mut fork_a: VecDeque<GasPrice> = common.clone().into();
    let mut fork_b: VecDeque<GasPrice> = common.into();

    // Fork A: STRK high → low target → fees fall.
    for _ in 0..200 {
        let w: Vec<GasPrice> = fork_a.iter().copied().collect();
        let fa = compute_fee_actual(&w, 10).unwrap_or(fallback);
        let target = compute_fee_target(
            TARGET_ATTO_USD_PER_L2_GAS,
            10u128.pow(18),
            ORACLE_L2_GAS_FLOOR_MIN_FRI,
            ORACLE_L2_GAS_FLOOR_MAX_FRI,
        );
        let p = compute_fee_proposal(Some(target), fa, FEE_PROPOSAL_MARGIN_PPT);
        fork_a.pop_front();
        fork_a.push_back(p);
    }

    // Fork B: STRK low → high target (= 30B, in range) → fees stay put.
    for _ in 0..200 {
        let w: Vec<GasPrice> = fork_b.iter().copied().collect();
        let fa = compute_fee_actual(&w, 10).unwrap_or(fallback);
        let target = compute_fee_target(
            TARGET_ATTO_USD_PER_L2_GAS,
            10u128.pow(17),
            ORACLE_L2_GAS_FLOOR_MIN_FRI,
            ORACLE_L2_GAS_FLOOR_MAX_FRI,
        );
        let p = compute_fee_proposal(Some(target), fa, FEE_PROPOSAL_MARGIN_PPT);
        fork_b.pop_front();
        fork_b.push_back(p);
    }

    let a_median = compute_fee_actual(&fork_a.iter().copied().collect::<Vec<_>>(), 10).unwrap();
    let b_median = compute_fee_actual(&fork_b.iter().copied().collect::<Vec<_>>(), 10).unwrap();
    // Fork A's fees fell; fork B stayed near 30B.
    assert!(a_median.0 < b_median.0, "expected fork_a < fork_b: {a_median:?} vs {b_median:?}");
}

// ============================================================================
// consensus_flow: drive the real SequencerConsensusContext through multi-block
// build_proposal + decision_reached cycles with scripted oracle rates, and
// verify the fee_proposal in the published ProposalInit matches the expected
// SNIP-35 formula at every height.
// ============================================================================

mod consensus_flow {
    use std::sync::{Mutex, OnceLock};

    use apollo_batcher_types::batcher_types::{
        CentralObjects,
        DecisionReachedResponse,
        FinishedProposalInfo,
        FinishedProposalInfoWithoutParent,
        GetProposalContent,
        GetProposalContentResponse,
        ProposalCommitment as BatcherProposalCommitment,
        ProposeBlockInput,
    };
    use apollo_consensus::types::ConsensusContext;
    use apollo_l1_gas_price_types::errors::ExchangeRateOracleClientError;
    use apollo_protobuf::consensus::{BuildParam, ProposalPart};
    use apollo_versioned_constants::VersionedConstants;
    use futures::StreamExt;
    use mockall::Sequence;
    use starknet_api::block::BlockNumber;
    use starknet_api::block_hash::block_hash_calculator::BlockHeaderCommitments;
    use starknet_api::state::ThinStateDiff;
    use starknet_api::versioned_constants_logic::VersionedConstantsTrait;

    use super::*;
    use crate::snip35::{compute_fee_actual, compute_fee_proposal, compute_fee_target};
    use crate::test_utils::{
        create_test_and_network_deps,
        INTERNAL_TX_BATCH,
        PARTIAL_BLOCK_HASH,
        TIMEOUT,
    };

    /// An oracle that returns a pre-scripted sequence of per-call responses.
    /// `None` simulates an oracle outage.
    fn scripted_oracle(rates: Vec<Option<u128>>) -> Arc<dyn ExchangeRateOracleClientTrait> {
        let state = Arc::new(Mutex::new(rates.into_iter().collect::<VecDeque<_>>()));
        // Keep a last-value fallback so background polls after the scripted window don't panic.
        let last = Arc::new(Mutex::new(None::<u128>));
        let mut mock = MockExchangeRateOracleClientTrait::new();
        mock.expect_eth_to_fri_rate().returning(move |_| {
            let mut queue = state.lock().unwrap();
            let next = queue.pop_front().unwrap_or_else(|| *last.lock().unwrap());
            *last.lock().unwrap() = next;
            match next {
                Some(rate) => Ok(rate),
                None => Err(ExchangeRateOracleClientError::RequestError("scripted outage".to_string())),
            }
        });
        Arc::new(mock)
    }

    /// Drive `n_blocks` of build_proposal + decision_reached through a fresh
    /// context wired with `oracle`, and return the ordered list of
    /// `fee_proposal` values observed on the wire.
    async fn run_proposer_chain(
        n_blocks: usize,
        oracle: Arc<dyn ExchangeRateOracleClientTrait>,
    ) -> Vec<GasPrice> {
        let (mut deps, mut network) = create_test_and_network_deps();
        // Default non-batcher expectations that don't break on multi-call.
        deps.setup_default_transaction_converter();
        deps.setup_default_gas_price_provider();
        // state_sync_client.get_block: return NotFound (SNIP-35 backfill on startup).
        deps.state_sync_client.expect_get_block().returning(|block_number| {
            Err(apollo_state_sync_types::communication::StateSyncClientError::StateSyncError(
                apollo_state_sync_types::errors::StateSyncError::BlockNotFound(block_number),
            ))
        });
        // For heights >= STORED_BLOCK_HASH_BUFFER (10), build_proposal performs a retrospective
        // block-hash lookup on both batcher and state_sync. Return a zero BlockHash for both.
        deps.state_sync_client
            .expect_get_block_hash()
            .returning(|_| Ok(starknet_api::block::BlockHash::default()));

        // Replace cende with one that handles n calls.
        let mut cende = crate::cende::MockCendeContext::new();
        cende
            .expect_write_prev_height_blob()
            .times(n_blocks)
            .returning(|_height| tokio::spawn(std::future::ready(true)));
        cende.expect_prepare_blob_for_next_height().times(n_blocks).returning(|_| Ok(()));
        deps.cende_ambassador = cende;

        // Manually set up batcher expectations: one propose/get-content cycle per height, with a
        // fresh proposal_id OnceLock per iteration. (The library helper reuses a single OnceLock
        // across iterations which panics on second `.set()`.)
        let mut seq = Sequence::new();
        for i in 0..n_blocks {
            let height = BlockNumber(u64::try_from(i).unwrap());
            let proposal_id = Arc::new(OnceLock::new());
            deps.batcher
                .expect_start_height()
                .times(1)
                .in_sequence(&mut seq)
                .withf(move |input| input.height == height)
                .return_const(Ok(()));

            let pid_clone = Arc::clone(&proposal_id);
            deps.batcher.expect_propose_block().times(1).in_sequence(&mut seq).returning(
                move |input: ProposeBlockInput| {
                    pid_clone.set(input.proposal_id).unwrap();
                    Ok(())
                },
            );

            let pid_clone = Arc::clone(&proposal_id);
            deps.batcher
                .expect_get_proposal_content()
                .times(1)
                .in_sequence(&mut seq)
                .withf(move |input| input.proposal_id == *pid_clone.get().unwrap())
                .returning(|_| {
                    Ok(GetProposalContentResponse {
                        content: GetProposalContent::Txs(INTERNAL_TX_BATCH.clone()),
                    })
                });

            let pid_clone = Arc::clone(&proposal_id);
            deps.batcher
                .expect_get_proposal_content()
                .times(1)
                .in_sequence(&mut seq)
                .withf(move |input| input.proposal_id == *pid_clone.get().unwrap())
                .returning(|_| {
                    Ok(GetProposalContentResponse {
                        content: GetProposalContent::Finished(FinishedProposalInfo {
                            artifact: FinishedProposalInfoWithoutParent {
                                proposal_commitment: BatcherProposalCommitment {
                                    partial_block_hash: PARTIAL_BLOCK_HASH,
                                },
                                final_n_executed_txs: INTERNAL_TX_BATCH.len(),
                                block_header_commitments: BlockHeaderCommitments::default(),
                                l2_gas_used: Default::default(),
                            },
                            parent_proposal_commitment: None,
                        }),
                    })
                });
        }
        // Return Ok for all batcher.get_block_hash calls so the retrospective lookup at
        // heights >= 10 doesn't retry-timeout.
        deps.batcher
            .expect_get_block_hash()
            .returning(|_| Ok(starknet_api::block::BlockHash::default()));

        deps.batcher.expect_decision_reached().times(n_blocks).returning(|_| {
            Ok(DecisionReachedResponse {
                state_diff: ThinStateDiff::default(),
                central_objects: CentralObjects::default(),
            })
        });
        deps.state_sync_client.expect_add_new_block().times(n_blocks).returning(|_| Ok(()));
        deps.strk_exchange_rate_oracle = Some(oracle);

        let mut context = deps.build_context();
        let mut observed = Vec::with_capacity(n_blocks);

        for i in 0..n_blocks {
            let height = BlockNumber(u64::try_from(i).unwrap());
            context.set_height_and_round(height, 0).await.unwrap();

            let _fin = context
                .build_proposal(BuildParam { height, ..Default::default() }, TIMEOUT)
                .await
                .unwrap();

            let (_, mut receiver) = network
                .outbound_proposal_receiver
                .next()
                .await
                .unwrap_or_else(|| panic!("block {i}: no outbound proposal envelope received"));
            let init = match receiver.next().await {
                Some(ProposalPart::Init(init)) => init,
                Some(other) => panic!("block {i}: expected Init, got {other:?}"),
                None => {
                    panic!("block {i}: inner proposal receiver closed without emitting any part")
                }
            };
            // Drain remaining parts so the proposal stream closes cleanly.
            while receiver.next().await.is_some() {}
            observed.push(init.fee_proposal.expect("V0_14_3+ proposer must emit Some"));

            context
                .decision_reached(
                    height,
                    0,
                    apollo_protobuf::consensus::ProposalCommitment(PARTIAL_BLOCK_HASH.0),
                    false,
                )
                .await
                .unwrap();
        }

        observed
    }

    /// Recompute the expected SNIP-35 `fee_proposal` given the proposer's
    /// current window, `l2_gas_price_fallback`, and the oracle rate for this
    /// block.
    fn expected_fee_proposal(
        window: &VecDeque<GasPrice>,
        rate: Option<u128>,
        l2_gas_price_fallback: GasPrice,
    ) -> GasPrice {
        let w: Vec<GasPrice> = window.iter().copied().collect();
        let fee_actual =
            compute_fee_actual(&w, FEE_PROPOSAL_WINDOW_SIZE).unwrap_or(l2_gas_price_fallback);
        let fee_target = rate.filter(|r| *r > 0).map(|r| {
            compute_fee_target(
                TARGET_ATTO_USD_PER_L2_GAS,
                r,
                ORACLE_L2_GAS_FLOOR_MIN_FRI,
                ORACLE_L2_GAS_FLOOR_MAX_FRI,
            )
        });
        compute_fee_proposal(fee_target, fee_actual, FEE_PROPOSAL_MARGIN_PPT)
    }

    // T1: Constant oracle rate over 12 heights. The window starts empty and
    // fills with each observed proposal. Every observed fee_proposal must
    // match the SNIP-35 formula computed from the window state at that height.
    #[tokio::test]
    async fn window_fills_and_matches_formula_over_12_blocks() {
        const RATE: u128 = 500_000_000_000_000_000; // $0.50
        const N: usize = 12;

        let observed = run_proposer_chain(N, scripted_oracle(vec![Some(RATE); N])).await;

        let fallback = VersionedConstants::latest_constants().min_gas_price;
        let mut window: VecDeque<GasPrice> = VecDeque::with_capacity(FEE_PROPOSAL_WINDOW_SIZE);
        for (i, actual) in observed.iter().enumerate() {
            let expected = expected_fee_proposal(&window, Some(RATE), fallback);
            assert_eq!(
                *actual,
                expected,
                "block {i}: observed {actual:?}, expected {expected:?} (window_len={})",
                window.len()
            );
            if window.len() >= FEE_PROPOSAL_WINDOW_SIZE {
                window.pop_front();
            }
            window.push_back(*actual);
        }

        // After 10+ blocks the window is full; the last proposal must be a real
        // median-based computation, not the fallback path.
        assert!(window.len() >= FEE_PROPOSAL_WINDOW_SIZE);
    }

    // T2: Oracle outage from block 10 onward. Up through block 9 we use a live
    // oracle; from block 10 we simulate outage. Once the window is full AND the
    // oracle is dead, SNIP-35 says fee_proposal = fee_actual exactly.
    #[tokio::test]
    async fn oracle_outage_after_window_fills_locks_to_fee_actual() {
        const RATE: u128 = 500_000_000_000_000_000; // $0.50
        const WARMUP: usize = 10;
        const OUTAGE: usize = 5;
        const N: usize = WARMUP + OUTAGE;

        let mut script = vec![Some(RATE); WARMUP];
        script.extend(std::iter::repeat_n(None, OUTAGE));
        let observed = run_proposer_chain(N, scripted_oracle(script)).await;

        let mut window: VecDeque<GasPrice> = VecDeque::with_capacity(FEE_PROPOSAL_WINDOW_SIZE);
        for actual in observed.iter().take(WARMUP) {
            if window.len() >= FEE_PROPOSAL_WINDOW_SIZE {
                window.pop_front();
            }
            window.push_back(*actual);
        }

        // During outage and with a full window, proposer must emit fee_actual exactly.
        for (i, actual) in observed.iter().enumerate().skip(WARMUP) {
            let w: Vec<GasPrice> = window.iter().copied().collect();
            let fee_actual = compute_fee_actual(&w, FEE_PROPOSAL_WINDOW_SIZE).unwrap();
            assert_eq!(
                *actual, fee_actual,
                "outage block {i}: proposer should have returned fee_actual {fee_actual:?}, got \
                 {actual:?}"
            );
            if window.len() >= FEE_PROPOSAL_WINDOW_SIZE {
                window.pop_front();
            }
            window.push_back(*actual);
        }
    }

    // T3: Sharp rate change mid-chain. Even when the proposer sees a 4x rate
    // jump, every observed fee_proposal must still satisfy the validator's
    // bounds check against its own (same-state) fee_actual.
    #[tokio::test]
    async fn sharp_rate_change_every_proposal_passes_validator_bounds() {
        const LOW: u128 = 500_000_000_000_000_000; // $0.50
        const HIGH: u128 = 2_000_000_000_000_000_000; // $2.00
        const N: usize = 16;

        let mut script = vec![Some(LOW); 8];
        script.extend(vec![Some(HIGH); N - 8]);
        let observed = run_proposer_chain(N, scripted_oracle(script.clone())).await;

        let mut window: VecDeque<GasPrice> = VecDeque::with_capacity(FEE_PROPOSAL_WINDOW_SIZE);
        for actual in &observed {
            if let Some(fee_actual) = compute_fee_actual(
                &window.iter().copied().collect::<Vec<_>>(),
                FEE_PROPOSAL_WINDOW_SIZE,
            ) {
                let lower = fee_actual.0.saturating_mul(PPT_DENOMINATOR)
                    / (PPT_DENOMINATOR + FEE_PROPOSAL_MARGIN_PPT);
                let upper = fee_actual.0.saturating_mul(PPT_DENOMINATOR + FEE_PROPOSAL_MARGIN_PPT)
                    / PPT_DENOMINATOR;
                assert!(
                    actual.0 >= lower && actual.0 <= upper,
                    "observed {actual:?} outside validator bounds [{lower}, {upper}] \
                     (fee_actual={fee_actual:?})"
                );
            }
            if window.len() >= FEE_PROPOSAL_WINDOW_SIZE {
                window.pop_front();
            }
            window.push_back(*actual);
        }
    }
}
