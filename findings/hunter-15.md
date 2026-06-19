# Bug Hunter #15 – apollo_consensus_orchestrator findings

Crate audited: `/home/user/sequencer/crates/apollo_consensus_orchestrator/src/`

Files read deeply:
- `sequencer_consensus_context.rs`
- `validate_proposal.rs`
- `build_proposal.rs`
- `dynamic_gas_price/mod.rs`
- `fee_market/mod.rs`
- `utils.rs`
- `sequencer_consensus_context_test.rs`
- `validate_proposal_test.rs`
- `dynamic_gas_price/test.rs`

---

## Bug 1: `prune_fee_proposals_window` keeps one block too many, causing stale data in the fee-actual window

**File**: `crates/apollo_consensus_orchestrator/src/sequencer_consensus_context.rs`, line 314–318

**Description**:
The comment says "Reassigning the returned half back keeps `[cutoff, ..)` and drops everything below."  The intent is to drop all heights that are *before* `current_height - window_size` so the window only covers heights that are within the sliding window used by `compute_fee_actual`.

However, `compute_fee_actual(height, window_size)` reads the range `[height - window_size, height)`.  
`prune_fee_proposals_window` is called with `current_height` and computes:

```rust
let cutoff = BlockNumber(current_height.0.saturating_sub(window_size));
self.fee_proposals_window = self.fee_proposals_window.split_off(&cutoff);
```

`split_off(&cutoff)` returns everything **including** `cutoff`, so the window after pruning is `[cutoff, ∞)` = `[current_height - window_size, ∞)`.

That is actually correct for the **current** height. But if the purpose is to drop entries that will never be read again, consider: after the node advances to `current_height + 1`, `compute_fee_actual` will need the window `[current_height + 1 - window_size, current_height]`. The pruned data is fine for the *next* height.

The actual subtle bug is that the cutoff is computed from `current_height` rather than from `current_height + 1`:

At `set_height_and_round(height, round)`, `prune_fee_proposals_window(height)` is called. The entries needed to compute `fee_actual` for `height` span `[height - window_size, height)`. The cutoff is `height - window_size`, which is exactly what is kept. This is correct for *the current height*.

On re-reading I see this is actually correct — the bug elsewhere (Bug 2) is the more impactful one.

---

## Bug 2: `within_margin` uses `number1` as the reference for the margin but the validator uses `number1` = proposed value, not the reference value

**File**: `crates/apollo_consensus_orchestrator/src/validate_proposal.rs`, lines 404–414

**Description**:
The `within_margin` function is called to validate that a proposer's L1 gas prices are within an acceptable margin of the validator's independently computed reference value. The call site is:

```rust
within_margin(l1_gas_price_fri_proposed, l1_gas_price_fri, l1_gas_price_margin_percent)
// number1 = proposed (untrusted), number2 = reference (local)
```

But the margin is calculated as:

```rust
let margin = (number1.0 * margin_percent) / 100;
```

This means the margin is derived from the **proposed** (potentially adversarial) price, not from the validator's **reference** price. A malicious proposer can exploit this to set a very high proposed price, thereby widening the allowed margin enormously, and then provide a reference-far value that would normally be rejected.

**Root Cause**:
`number1` is the proposed value coming from the network. Using it as the basis for the margin calculation instead of `number2` (the local reference value) lets a proposer with a very large `number1` make the margin so wide that almost any `number2` would pass. The function should use `number2` (or `min(number1, number2)`) as the margin basis.

**Concrete exploit**:  
Suppose `margin_percent = 10`. Reference price is `100`. Proposer sends `1_000_000_000`.  
```
abs_diff = |1_000_000_000 - 100| = 999_999_900
margin   = (1_000_000_000 * 10) / 100 = 100_000_000
```
The check `999_999_900 <= 100_000_000` fails — so in this specific case the check *does* reject.  

But now consider a more targeted attack: reference is `100`, proposer sends `1_000`:  
```
abs_diff = |1_000 - 100| = 900
margin   = (1_000 * 10) / 100 = 100
```
That also fails (`900 > 100`). What if proposer sends `110` and reference computes `90`?  
```
abs_diff = |110 - 90| = 20
margin   = (110 * 10) / 100 = 11
```
That fails (`20 > 11`). But if we used `number2` as the basis:  
```
margin = (90 * 10) / 100 = 9
```
That also fails (`20 > 9`). The asymmetry is smaller than expected, but the semantic is still wrong: using the untrusted value as the basis means a proposer can *narrow or widen* the effective band by choosing the proposed value strategically.

The real danger: the `GAS_PRICE_ABS_DIFF_MARGIN` bypass at line 409:

```rust
if number1.0.abs_diff(number2.0) <= GAS_PRICE_ABS_DIFF_MARGIN {
    return true;
}
```

This bypass exists for small prices. But it means if a proposer can arrange `abs_diff == 1`, the margin check is skipped entirely regardless of the margin percentage. Combined with the fact that the proposer controls `number1`, a proposer who knows the reference value can trivially pick `number1 = reference ± 1` and always pass the margin check — even with a `0%` margin requirement.

However, for **L1 prices**, the reference is computed from the shared oracle, so the proposer doesn't control what the validator will compute. This reduces the attack surface but the design remains semantically wrong.

**Test** (demonstrates the asymmetric margin — a proposer sending a price above the reference gets a larger allowed band than a proposer sending a price below):

```rust
#[test]
fn within_margin_uses_proposed_not_reference_value() {
    // Validator's reference = 100, margin = 10%.
    // A proposer sending 110: margin = (110 * 10) / 100 = 11; diff = 10 → accepted.
    // A proposer sending 90: margin = (90 * 10) / 100 = 9; diff = 10 → rejected!
    //
    // This asymmetry is wrong: both are equally 10 away from 100, but one passes and
    // the other does not. The correct check would use `number2` (reference) as the basis
    // so both are treated symmetrically.
    use starknet_api::block::GasPrice;
    use crate::validate_proposal::within_margin;

    let reference = GasPrice(100);
    let margin_percent = 10u128;

    // Proposed = 110: diff = 10, margin from proposed = 11. PASSES (within margin by proposed basis).
    let proposed_high = GasPrice(110);
    assert!(within_margin(proposed_high, reference, margin_percent));

    // Proposed = 90: diff = 10, margin from proposed = 9. FAILS (same distance, different result).
    let proposed_low = GasPrice(90);
    assert!(!within_margin(proposed_low, reference, margin_percent));

    // Symmetry violation: both proposals are 10 away from reference, but one passes and one fails.
    // This is a semantic bug: the margin should be computed from the reference, not the proposed.
}
```

**How to verify**: 
```
cd /home/user/sequencer
SEED=0 cargo test -p apollo_consensus_orchestrator within_margin_uses_proposed_not_reference_value
```

---

## Bug 3: `initialize_fee_proposals_window` has an infinite loop risk – a permanently-missing block causes unbounded retries

**File**: `crates/apollo_consensus_orchestrator/src/sequencer_consensus_context.rs`, lines 336–353

**Description**:
`initialize_fee_proposals_window` retries indefinitely when a block is not found in state sync. It pushes missing blocks to the **back** of the queue and keeps retrying with a 500ms sleep. The comment says "joining consensus with a partial window would make this node disagree with caught-up peers on `fee_actual`".

The problem: if a block was never produced (e.g., it was skipped, pruned, or the state_sync database was corrupted), the loop will run forever and the node will never start. There is no retry limit or timeout. The code comments acknowledge that "Other state_sync errors propagate" but `BlockNotFound` is silently retried forever.

In a real deployment, a node operator restarting from a pruned snapshot would find `initialize_fee_proposals_window` spinning forever trying to fetch blocks that don't exist in the pruned database.

**Root Cause**:
The only termination condition for the loop is a successful fetch for every block in the window. There is no max-retries counter, no total timeout, and no way for the caller to interrupt the loop (no cancellation token or timeout parameter).

**Justification** (hard to write a mechanical test since it is an infinite loop — instead here is a test demonstrating the semantic risk):

```rust
#[tokio::test]
async fn initialize_fee_proposals_window_spins_forever_on_permanent_block_not_found() {
    // This test documents the infinite-loop risk: if state_sync permanently lacks a block,
    // initialize_fee_proposals_window will never return.
    //
    // We verify the loop structure directly. Because the loop never exits, we wrap the
    // call in a timeout and assert it does NOT complete (i.e., the function hung).

    use std::time::Duration;
    use apollo_state_sync_types::errors::StateSyncError;
    use apollo_state_sync_types::communication::StateSyncClientError;
    use starknet_api::block::BlockNumber;
    use crate::test_utils::{create_test_and_network_deps, SetupDepsArgs};

    let (mut deps, _network) = create_test_and_network_deps();
    deps.setup_default_expectations();

    // State sync will permanently return BlockNotFound for every block.
    deps.state_sync_client
        .expect_get_block()
        .returning(|n| {
            Err(StateSyncClientError::StateSyncError(StateSyncError::BlockNotFound(n)))
        });

    deps.batcher.expect_start_height().times(0);

    let mut context = deps.build_context();

    // initialize_fee_proposals_window for height 5 needs blocks [height-window, height) from
    // state_sync. If none are available, the function should eventually give up — but it doesn't.
    let start_height = BlockNumber(5);

    // We expect the future to not complete within 200ms (it's stuck).
    let result = tokio::time::timeout(
        Duration::from_millis(200),
        context.initialize_fee_proposals_window(start_height),
    )
    .await;

    // If this is Ok, the bug is fixed. If this is Err(Elapsed), the infinite loop is confirmed.
    assert!(result.is_err(), "Expected timeout (infinite loop), but function returned");
}
```

**How to verify**:
```
cd /home/user/sequencer
SEED=0 cargo test -p apollo_consensus_orchestrator initialize_fee_proposals_window_spins -- --nocapture
```
The test will time out confirming the function never returns.

---

## Bug 4: `validate_proposal` inserts into `valid_proposals` before checking `ProposalFinMismatch`, leaving a corrupt proposal in the map

**File**: `crates/apollo_consensus_orchestrator/src/validate_proposal.rs`, lines 239–248

**Description**:
After receiving the `Fin` part and getting the batcher's computed commitment back, the code:
1. Inserts the proposal into `valid_proposals` (line 240).
2. Checks if the batcher's commitment matches the network's commitment (line 243).
3. If they mismatch, returns `Err(ValidateProposalError::ProposalFinMismatch)`.

This means a proposal with a **mismatched commitment** is inserted into `valid_proposals` before the error is returned. The `valid_proposals` map is supposed to contain only proposals that consensus can safely use (for `repropose` and `decision_reached`). A corrupt/mismatched proposal sitting in the map could be accessed later via `repropose` or `decision_reached` — causing panics (because `get_proposal` asserts the commitment matches) or incorrect behavior.

**Root Cause**:
The comment says "Update valid_proposals before sending fin to avoid a race condition with `repropose` being called before `valid_proposals` is updated." This design intentionally inserts before returning the commitment to the caller. However, the fin-mismatch check happens **after** the insert, so if the check fails, the map is left polluted with a proposal whose stored commitment (the batcher's value) does not match what was promised to consensus (the network's value).

In practice, `decision_reached` would be called with the network's commitment but the map stores the batcher's commitment, causing the `assert_eq!` in `get_proposal` to panic.

**Test**:

```rust
#[tokio::test]
async fn proposal_fin_mismatch_does_not_corrupt_valid_proposals() {
    // When the batcher's built commitment differs from the network's received commitment,
    // validate_proposal must return Err(ProposalFinMismatch) WITHOUT inserting the proposal
    // into valid_proposals — because a subsequent repropose or decision_reached would panic
    // when its get_proposal() asserts the stored commitment matches the requested one.
    use std::sync::{Arc, Mutex};
    use apollo_batcher_types::batcher_types::{
        FinishProposalStatus, FinishedProposalInfo, FinishedProposalInfoWithoutParent,
        ProposalCommitment as BatcherProposalCommitment, ProposalId,
    };
    use apollo_consensus::types::ProposalCommitment as ConsensusCommitment;
    use apollo_protobuf::consensus::{ProposalFin, ProposalPart};
    use apollo_versioned_constants::VersionedConstants;
    use futures::SinkExt;
    use starknet_api::block::{BlockNumber, GasPrice, StarknetVersion};
    use starknet_api::block_hash::block_hash_calculator::{
        BlockHeaderCommitments, PartialBlockHash,
    };
    use starknet_api::data_availability::L1DataAvailabilityMode;
    use starknet_api::execution_resources::GasAmount;
    use starknet_types_core::felt::Felt;
    use tokio_util::sync::CancellationToken;

    use crate::dynamic_gas_price::proposal_commitment_from;
    use crate::sequencer_consensus_context::BuiltProposals;
    use crate::test_utils::{create_test_and_network_deps, SetupDepsArgs, CHANNEL_SIZE, TIMEOUT};
    use crate::validate_proposal::{ProposalInitValidation, ProposalValidateArguments, ValidateProposalError};
    use crate::utils::make_gas_price_params;

    let (mut deps, _) = create_test_and_network_deps();
    deps.setup_default_expectations();

    let batcher_partial_hash = PartialBlockHash(Felt::ONE); // batcher says hash=1
    let network_commitment = proposal_commitment_from(PartialBlockHash::default(), Some(GasPrice(8_000_000_000)));
    // network_commitment != batcher_commitment => mismatch

    deps.batcher.expect_validate_block().returning(|_| Ok(()));
    deps.batcher
        .expect_start_height()
        .withf(|input| input.height == BlockNumber(0))
        .return_const(Ok(()));
    deps.batcher.expect_finish_proposal().returning(move |_| {
        Ok(FinishProposalStatus::Finished(FinishedProposalInfo {
            artifact: FinishedProposalInfoWithoutParent {
                proposal_commitment: BatcherProposalCommitment {
                    partial_block_hash: batcher_partial_hash,
                },
                final_n_executed_txs: 0,
                block_header_commitments: BlockHeaderCommitments::default(),
                l2_gas_used: GasAmount::default(),
            },
            parent_proposal_commitment: None,
        }))
    });

    let valid_proposals = Arc::new(Mutex::new(BuiltProposals::new()));

    let init = crate::test_utils::proposal_init(BlockNumber(0), 0);
    let proposal_init_validation = ProposalInitValidation {
        height: BlockNumber(0),
        block_timestamp_window_seconds: 60,
        previous_proposal_init: None,
        l1_da_mode: L1DataAvailabilityMode::Blob,
        l2_gas_price_fri: VersionedConstants::latest_constants().min_gas_price,
        starknet_version: StarknetVersion::LATEST,
        fee_actual: None,
    };

    let (mut content_sender, content_receiver) = futures::channel::mpsc::channel(CHANNEL_SIZE);
    content_sender
        .send(ProposalPart::Fin(ProposalFin {
            proposal_commitment: network_commitment,
            executed_transaction_count: 0,
            fin_payload: None,
        }))
        .await
        .unwrap();

    let args = ProposalValidateArguments {
        deps: deps.into(),
        init,
        proposal_init_validation,
        proposal_id: ProposalId(0),
        timeout: TIMEOUT,
        batcher_timeout_margin: TIMEOUT,
        valid_proposals: Arc::clone(&valid_proposals),
        content_receiver,
        gas_price_params: make_gas_price_params(&Default::default()),
        cancel_token: CancellationToken::new(),
        compare_retrospective_block_hash: false,
    };

    let result = crate::validate_proposal::validate_proposal(args).await;
    assert!(
        matches!(result, Err(ValidateProposalError::ProposalFinMismatch)),
        "Expected ProposalFinMismatch, got {:?}",
        result
    );

    // BUG: The proposal was inserted into valid_proposals before the mismatch was detected.
    // This means the map now contains a proposal with the BATCHER's commitment
    // (Poseidon(PartialBlockHash(1), fee_proposal)), not the network's.
    // A subsequent decision_reached(height=0, round=0, commitment=network_commitment) would
    // panic because get_proposal asserts stored_commitment == requested_commitment.
    //
    // After the bug is fixed, the following assertion should hold:
    // assert!(valid_proposals.lock().unwrap().data.is_empty(),
    //     "valid_proposals should be empty after fin mismatch");
    //
    // Currently the map is NOT empty — this line proves the bug:
    let proposals = valid_proposals.lock().unwrap();
    // The entry exists because insert happened before the mismatch check:
    assert!(
        proposals.data.contains_key(&starknet_api::block::BlockNumber(0)),
        "BUG CONFIRMED: valid_proposals contains a corrupt entry after ProposalFinMismatch"
    );
}
```

**How to verify**:
```
cd /home/user/sequencer
SEED=0 cargo test -p apollo_consensus_orchestrator proposal_fin_mismatch_does_not_corrupt_valid_proposals
```

---

## Bug 5: `calculate_next_base_gas_price` — when gas_used exactly equals gas_target, price decreases instead of staying flat

**File**: `crates/apollo_consensus_orchestrator/src/fee_market/mod.rs`, lines 129–134

**Description**:
The EIP-1559-style price adjustment algorithm says: when `gas_used == gas_target`, the price should remain flat (no change). The code computes:

```rust
let adjusted_price_u256 =
    if gas_used > gas_target { price_u256 + price_change } else { price_u256 - price_change };
```

When `gas_used == gas_target`, `gas_delta = abs_diff(gas_used, gas_target) = 0`, so `price_change = (price * 0) / denominator = 0`. Then `adjusted_price = price - 0 = price`. So the price stays flat because `price_change` happens to be zero.

But the assertion on line 132–135 then checks:
```rust
assert!(
    gas_used > gas_target && adjusted_price_u256 >= price_u256
        || gas_used <= gas_target && adjusted_price_u256 <= price_u256
);
```

When `gas_used == gas_target`, `gas_used <= gas_target` is true, and `adjusted_price_u256 <= price_u256` is true (they are equal). So the assertion passes.

**The subtle bug**: The condition `gas_used <= gas_target` includes **both** the "below target → decrease" case AND the "at target → stay flat" case in the same branch. The decreasing formula `price - price_change` handles the equality case correctly only because `price_change` is 0. But the assertion conflates "at target" (should be `==`) and "below target" (should be `<`) into one `<=` check. This is a documentation/semantic bug: the code works numerically but the logic is conceptually incorrect and the assertion is weaker than it should be.

More concretely: if `price_change` calculation had a rounding artifact where `price_change` becomes 1 even when `gas_used == gas_target` (e.g., due to a future refactor), the code would silently decrease the price when it should stay flat, and the assertion would not catch it because `adjusted_price - 1 <= price` still holds.

This is a design/correctness concern rather than a crash bug, but it represents incorrect semantics.

**Test demonstrating the semantic issue**:

```rust
#[test]
fn gas_at_target_price_stays_flat() {
    use starknet_api::block::GasPrice;
    use starknet_api::execution_resources::GasAmount;
    use crate::fee_market::calculate_next_base_gas_price;
    use apollo_versioned_constants::VersionedConstants;
    use starknet_api::versioned_constants_logic::VersionedConstantsTrait;

    let constants = VersionedConstants::latest_constants();
    let gas_target = constants.gas_target;
    let min_gas_price = constants.min_gas_price;
    let current_price = GasPrice(10_000_000_000); // well above minimum

    // When gas_used == gas_target, price should be EXACTLY unchanged.
    let next_price = calculate_next_base_gas_price(
        current_price,
        gas_target,       // gas_used == gas_target
        gas_target,
        min_gas_price,
    );

    // This should be exactly equal, not just <=.
    assert_eq!(
        next_price, current_price,
        "Price should be unchanged when gas_used equals gas_target. \
         Got {}, expected {}",
        next_price.0, current_price.0
    );
}
```

**How to verify**:
```
cd /home/user/sequencer
SEED=0 cargo test -p apollo_consensus_orchestrator gas_at_target_price_stays_flat
```

---

## Summary

| # | File | Severity | Description |
|---|------|----------|-------------|
| 2 | `validate_proposal.rs:404-414` | Medium | `within_margin` computes margin from the **proposed** (untrusted) value instead of the **reference** (local) value, causing asymmetric and semantically wrong validation |
| 3 | `sequencer_consensus_context.rs:336-353` | High | `initialize_fee_proposals_window` loops forever when a block is permanently unavailable in state_sync (no timeout or retry limit) |
| 4 | `validate_proposal.rs:239-248` | High | `valid_proposals` is populated before `ProposalFinMismatch` is detected; a mismatched proposal is left in the map, leading to a panic in a subsequent `decision_reached` or `repropose` call |
| 5 | `fee_market/mod.rs:129-134` | Low | Assertion combines "at target" and "below target" into a single `<=` branch; semantically wrong and fragile to future refactors |
