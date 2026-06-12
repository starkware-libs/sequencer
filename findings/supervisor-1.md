# Supervisor 1 Report

## Hunter 1 — Bug 1

**Verdict**: confirmed

**Rationale**: Traced the logic in `crates/apollo_gateway/src/stateless_transaction_validator.rs`, `fn validate_resource_bounds` (lines 71–76). The code unconditionally checks:

```rust
if resource_bounds.l2_gas.max_price_per_unit.0 < self.config.min_gas_price {
    return Err(StatelessTransactionValidatorError::MaxGasPriceTooLow { … });
}
```

There is no guard for `l2_gas.max_amount == 0`. The production default config (`StatelessTransactionValidatorConfig::default()`) sets `min_gas_price: 8_000_000_000` and `validate_resource_bounds: true`. The `max_possible_fee` calculation in `ValidResourceBounds::AllResources` sums across all three gas types, so a transaction with `l1_gas = {max_amount=100, max_price=1_000_000_000}` and `l2_gas = {max_amount=0, max_price=0}` correctly passes the `ZeroResourceBounds` check (fee = 100 * 1_000_000_000 ≠ 0) but then trips the `MaxGasPriceTooLow` guard because `0 < 8_000_000_000`.

The bug is invisible in tests because `DEFAULT_VALIDATOR_CONFIG_FOR_TESTING` sets `min_gas_price: 0`, making the guard `0 < 0` = false. The proposed test uses the production `StatelessTransactionValidatorConfig::default()` and constructs the transaction through the normal public API — no internals are reached into. The test legitimately reflects how any user submitting an L1-only V3 transaction against a production gateway would encounter the rejection.

---

## Hunter 2 — Bug 1

**Verdict**: confirmed

**Rationale**: Traced the call chain in `crates/apollo_mempool/src/mempool.rs`. `add_tx` calls `add_tx_validations(tx_reference, &args.tx, args.account_state.nonce)` at line 468, passing the raw user-submitted nonce. `add_tx_validations` passes this same raw nonce to `handle_capacity_overflow(tx, account_nonce)` at line 421. Inside `handle_capacity_overflow` (lines 1011–1012):

```rust
let closing_gap = tx.nonce() == account_nonce;   // account_nonce is raw user-submitted
let creating_gap = (account_has_gap || !account_has_txs) && !closing_gap;
```

The `validate_incoming_tx` function (called earlier) does resolve the nonce internally via `self.resolve_nonce(address, incoming_account_nonce)` and validates `tx_nonce >= resolved_nonce`, but the resolved nonce is not propagated to `handle_capacity_overflow`. The capacity overflow check therefore compares the tx nonce against the stale raw nonce, not the mempool's view of the account nonce.

The scenario is realistic: a client submits a transaction with a stale `account_nonce` (they haven't yet observed a committed block) — this is a standard race condition in production. After a commit, `committed["address"] = 1`; a client submitting `account_nonce=0` (stale) passes nonce validation because `resolve_nonce(address, 0) = 1` and `tx_nonce >= 1` is checked. But `handle_capacity_overflow` sees raw `account_nonce=0` and misclassifies the gap-closing nonce=1 tx as gap-creating.

The test uses the standard `add_tx_input!` macro and public `add_tx`, `commit_block`, `get_txs_and_assert_expected` helpers. It constructs a scenario that real users would trigger through normal usage. The test is legitimate.

---

## Hunter 3 — Bug 1

**Verdict**: confirmed

**Rationale**: Traced `fn get_parent_proposal_commitment` in `crates/apollo_batcher/src/batcher.rs` (lines 1359–1401). When `prev_proposal_commitment` is `None`, the function reads `get_parent_hash_and_partial_block_hash_components(prev_height)` from storage and calls `.expect("Missing partial block hash components for previous height.")` on the result.

When an old-protocol block is committed via `add_sync_block`, `StorageCommitmentBlockHash::ParentHash(block_header_without_hash.parent_hash)` is used. The `commit_proposal` storage writer path for `ParentHash` (lines 1848–1851) calls `set_block_hash` on the parent but never calls `set_partial_block_hash_components`. Therefore `get_partial_block_hash_components(prev_height)` returns `None` for old-protocol blocks.

After committing an old-protocol block via `add_sync_block`, `commit_proposal_and_block` sets `self.prev_proposal_commitment = None` (line 1065+1077 logic: `proposal_commitment = None` for `ParentHash` variant). When `decision_reached` is subsequently called for the first new-protocol block (line 950), it calls `get_parent_proposal_commitment(height)` → falls into the `None` branch → storage lookup → `components = None` → panic.

The test uses `MockBatcherStorageReader` and mocks `get_parent_hash_and_partial_block_hash_components` to return `(Some(BlockHash::default()), None)` — precisely the response the real storage gives for old-protocol blocks. This is not manufactured; the mock accurately models the real storage state. The test is legitimate and demonstrates a crash path that occurs at the protocol transition boundary for any node that synced old blocks and then begins producing new-protocol blocks.

---

## Hunter 3 — Bug 2

**Verdict**: suspected

**Rationale**: The race condition is structurally present in the code. In `abort_proposal` (lines 642–657), `is_active(proposal_id).await` acquires and releases the `active_proposal` mutex, then `abort_active_proposal().await` re-acquires it and takes the value. The spawned execution task (lines 1188–1199) separately acquires `active_proposal`, takes the value, then acquires `executed_proposals` and inserts the result — these are two sequential lock acquisitions with no atomicity between them. In a multi-threaded tokio runtime, the spawned task can complete between `is_active` returning `true` and `abort_active_proposal` reacquiring the mutex, resulting in the task inserting into `executed_proposals` before `abort_proposal` does, causing the `assert!(proposal_already_exists.is_none(), ...)` panic at line 653.

However, the proposed test acknowledges the race is non-deterministic: "sometimes it panics, sometimes it succeeds depending on scheduling." The test uses `tokio::task::yield_now().await` to narrow the scheduling window, but the assert documents that reproducibility is not guaranteed. In single-threaded tokio (the default for `#[tokio::test]`), await points are fully cooperatively scheduled and the race cannot fire because there is no await point between `is_active` returning true and the insert into `executed_proposals`. The test as written will most likely not fail, making it an artificial reproduction attempt rather than a reliable demonstration.

The underlying design flaw is real and would manifest in production (multi-threaded tokio), but the test does not reliably demonstrate it.

---

## Hunter 3 — Bug 3

**Verdict**: suspected

**Rationale**: The `panic!` at line 595 in `send_txs_for_proposal` is present in production code:

```rust
Ok(_) => panic!("Proposal finished validation before all transactions were sent."),
```

This can be reached in normal operation: a validator's block builder may complete (hit time deadline, transaction limit, or any early-exit condition) before the proposer finishes streaming transactions. The comment says "should not occur in normal protocol flow," but this is a design assumption rather than a guarantee — the proposer and validator operate independently and the timing is not synchronized. A panic here crashes the entire node process rather than returning a graceful error.

However, no standalone test is provided. The hunter states the path would be triggered by combining with the Bug 2 race condition, making the test contingent on an already-suspected bug. The bug is a legitimate code quality and reliability issue but cannot be independently verified or demonstrated without a failing test.

---

## Hunter 4 — Bug 1

**Verdict**: confirmed

**Rationale**: Traced `fn maybe_advance_to_round` in `crates/apollo_consensus/src/state_machine.rs` (lines 720–728):

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

The Tendermint paper (Algorithm 1, line 55) conditions round advancement on receiving messages of **any** type from validators with combined weight > f (i.e., `f+1` messages from distinct validators, where f = 1/3 of stake). The implementation checks prevotes OR precommits separately rather than summing their weights.

Using `ROUND_SKIP_THRESHOLD = VotesThreshold::new(1, 3)` and `is_met` semantics (`amount * 3 > total * 1`): with total=4, threshold requires `amount * 3 > 4` → `amount >= 2`. One prevote (weight=1) alone: `1*3=3 > 4` = false. One precommit (weight=1) alone: same = false. Combined (weight=2): `2*3=6 > 4` = true. The node should advance but does not.

The test uses `send_prevote_from` and `send_precommit_from` — standard public event-injection helpers already present in `state_machine_test.rs`. It injects one prevote and one precommit for a future round from two different validators. This reflects exactly how a real network would send round-skip signals: honest validators might independently send prevotes or precommits for the next round, and their combined weight should trigger advancement. The test is legitimate and demonstrates a real liveness violation where a node cannot advance to the next round even after receiving sufficient combined signal from honest validators.

---

## Summary

- Confirmed: 4 bugs (Hunter 1 Bug 1, Hunter 2 Bug 1, Hunter 3 Bug 1, Hunter 4 Bug 1)
- Suspected: 2 bugs (Hunter 3 Bug 2, Hunter 3 Bug 3)
- Rejected: 0 bugs
