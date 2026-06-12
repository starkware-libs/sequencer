# Bug Hunter 4 Findings

## Files Examined

- `crates/apollo_consensus/src/votes_threshold.rs` — quorum threshold arithmetic
- `crates/apollo_consensus/src/votes_threshold_test.rs` — threshold tests
- `crates/apollo_consensus/src/state_machine.rs` — full state machine (LOC-by-LOC vs Tendermint paper)
- `crates/apollo_consensus/src/state_machine_test.rs` — all unit tests including weighted-quorum tests
- `crates/apollo_consensus/src/single_height_consensus.rs` — SHC wrapper
- `crates/apollo_consensus/src/single_height_consensus_test.rs` — SHC tests
- `crates/apollo_consensus/src/simulation_test.rs` — discrete-event simulation test
- `crates/apollo_consensus/src/manager.rs` — multi-height manager, caching, vote routing
- `crates/apollo_consensus/src/manager_test.rs` — manager tests
- `crates/apollo_consensus/src/test_utils.rs` — test helpers
- `crates/apollo_consensus/src/storage.rs` — height-voted storage
- `crates/apollo_consensus_manager/src/consensus_manager.rs` — top-level component
- `crates/apollo_consensus_config/src/config.rs` — timeout and limit config

---

## Bug 1

**File**: `crates/apollo_consensus/src/state_machine.rs`  
**Location**: `fn maybe_advance_to_round`, line ~720  
**Description**: Round-skip threshold (LOC 55 of the Tendermint paper) is evaluated per vote type rather than across the combined weight of all vote types for the target round.

**Root Cause**: The Tendermint paper (Algorithm 1, line 55) states:

> upon f+1 ⟨*, h_p, r, *, *⟩ with r > round_p

The leading `*` means "any message type" — prevote or precommit are pooled together. A node should advance to round `r` as soon as it has received messages from validators whose combined weight exceeds f (i.e., more than 1/3 of total stake).

The implementation checks prevote weight *or* precommit weight in isolation:

```rust
fn maybe_advance_to_round(&mut self, round: u32) -> VecDeque<SMRequest> {
    if self.round_has_enough_votes(&self.prevotes, round, &self.round_skip_threshold)
        || self.round_has_enough_votes(&self.precommits, round, &self.round_skip_threshold)
    {
        self.advance_to_round(round)
    } else {
        VecDeque::new()
    }
}
```

This means that if honest validators split their round-skip signals between prevotes and precommits (e.g., validator A prevotes for round r and validator B precommits for round r), the node never advances even though the combined weight exceeds f+1. The correct check is: sum the weights across both prevotes **and** precommits for the target round.

**Concrete example** (4 validators, each weight 1, total = 4, f = 1, threshold = f+1 = 2 votes):
- Validator A prevotes for round 1 (weight 1)
- Validator B precommits for round 1 (weight 1)
- Combined weight = 2 > 4/3 ≈ 1.33 → should advance per the paper
- Code: `round_has_enough_votes(prevotes, 1, threshold)` = `is_met(1, 4)` = `3 > 4` = false; `round_has_enough_votes(precommits, 1, threshold)` = `is_met(1, 4)` = `3 > 4` = false
- Result: **no round advancement** — liveness violation

**Failing Test**:

Add to `crates/apollo_consensus/src/state_machine_test.rs`, inside the existing `mod state_machine_test`:

```rust
/// Demonstrates Bug 1: round-skip threshold checked per vote type instead of combined.
///
/// With 4 validators each of weight 1 (total = 4, f = 1, skip threshold = f+1 = 2),
/// one prevote and one precommit for a future round together cross the threshold, but
/// each alone does not. The state machine must advance when their *combined* weight
/// exceeds 1/3 of total stake — it currently does not.
#[test]
fn round_skip_threshold_must_combine_prevote_and_precommit_weight() {
    // 4 validators, each weight 1. Round-skip needs > 4/3 combined weight, so weight 2.
    let mut wrapper = TestWrapper::new(
        *VALIDATOR_ID,
        UNIT_VALIDATOR_WEIGHTS.clone(),
        |_: Round| *PROPOSER_ID,
        |_: Round| Ok(*PROPOSER_ID),
        QuorumType::Byzantine,
        NOT_OBSERVER,
    );

    advance_validator_after_start(&mut wrapper);

    // Send one prevote for future round 1 from VALIDATOR_ID_2 (weight 1).
    // This alone is not enough to trigger the round-skip (need combined weight > 4/3).
    wrapper.send_prevote_from(PROPOSAL_ID, ROUND + 1, *VALIDATOR_ID_2);
    assert_no_more_requests(&mut wrapper);

    // Send one precommit for future round 1 from VALIDATOR_ID_3 (weight 1).
    // Combined weight of prevote + precommit = 2 > 4/3, so the Tendermint paper (LOC 55)
    // requires the node to advance to round 1.
    //
    // BUG: the current implementation checks each vote type separately and never
    // advances. This assertion will FAIL until the bug is fixed.
    wrapper.send_precommit_from(PROPOSAL_ID, ROUND + 1, *VALIDATOR_ID_3);
    assert_eq!(
        wrapper.next_request().unwrap(),
        SMRequest::ScheduleTimeout(Step::Propose, ROUND + 1),
        "Expected round advancement to round 1 after combined prevote+precommit weight \
         exceeds round-skip threshold, but no advancement occurred"
    );
    assert_eq!(wrapper.round(), ROUND + 1);
}
```

**How to Verify**: `SEED=0 cargo test -p apollo_consensus round_skip_threshold_must_combine_prevote_and_precommit_weight`

The test will **fail** because after receiving one prevote and one precommit for round 1, `maybe_advance_to_round` returns `VecDeque::new()` instead of advancing. The fix is to sum both vote types' weights before comparing to the threshold.

---

## Areas Checked but No Bug Found

The following areas were thoroughly analyzed and found to be correctly implemented:

1. **`VotesThreshold::is_met`** — strict `>` comparison is correct for "more than 2/3" and "more than 1/3". The 2-out-of-3 boundary case (exactly 2/3 is NOT enough) is properly enforced and explicitly tested.

2. **`upon_new_proposal` (LOC 22)** — correctly guards on `locked_value == proposal OR not locked`.

3. **`upon_reproposal` (LOC 28)** — correctly guards on `locked_round ≤ valid_round OR locked_value == v`, with step deduplication via `step != Propose` early return.

4. **`upon_prevote_quorum` (LOC 36)** — correctly uses `prevote_quorum` HashSet per round for idempotency; updates `valid_value_round` and `locked_value_round` correctly only in Prevote step.

5. **`upon_nil_prevote_quorum` (LOC 44)** — deduplication via `advance_to_step(Precommit)` is sufficient; subsequent calls return early due to `step != Prevote`.

6. **`upon_decision` (LOC 49)** — correctly triggered for both current and past rounds; precommit set guaranteed non-empty.

7. **`awaiting_finished_building` buffering** — correctly prevents round advancement while building a proposal; stale `FinishedBuilding` events cannot arrive from the same `StartBuildProposal` since the build future can only complete once.

8. **Vote deduplication in SHC** — `received_vote` checks both the state machine maps and the events queue, preventing duplicate votes from reaching `handle_prevote`/`handle_precommit`.

9. **`ConsensusCache`** — future vote/proposal caching correctly bounds height and round windows; `get_current_height_votes` correctly removes entries ≤ current height.

10. **Quorum threshold for HONEST and BYZANTINE modes** — correct for all test-case committee sizes examined.
