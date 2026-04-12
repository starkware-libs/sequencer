# Separate Mempool Reset-Staged from Commit History Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a `reset_staged()` method to the Mempool that resets staged nonces for a new proposal without consuming a `CommitHistory` ring buffer slot, fixing history dilution caused by `Batcher::propose_block` calling `commit_block(CommitBlockArgs::default())`.

**Architecture:** Extract the shared "drain staged" logic from `MempoolState::commit` into a private `drain_staged` helper used by both `commit` and a new `reset_staged`. Expose `Mempool::reset_staged` as a public method, wire it through the communication layer, then update `Batcher::propose_block` to call it. Three PRs: business logic, interface, batcher usage.

**Tech Stack:** Rust, async_trait, mockall, apollo infra component communication layer (`handle_all_response_variants!` macro, `MempoolRequest`/`MempoolResponse` enums).

---

## Background

`CommitHistory` is a fixed-size ring buffer (`VecDeque<AddressToNonce>`, capacity = `committed_nonce_retention_block_count`, default 100). Every call to `MempoolState::commit` — regardless of whether `address_to_nonce` is empty — pushes one entry and evicts one from the front.

`Batcher::propose_block` calls `commit_block(CommitBlockArgs::default())` to reset staged nonces before starting a new proposal. This pushes an empty entry and evicts a real one, halving effective nonce retention in normal single-round operation (1 empty + 1 real per block height = 50 effective blocks instead of 100). Multi-round consensus makes it worse.

The fix: introduce `reset_staged()`, which does only the staged-reset + queue-rewind work, with no ring buffer interaction.

---

## Files to Touch

| File | Change |
|------|--------|
| `crates/apollo_mempool/src/mempool.rs` | Extract `MempoolState::drain_staged`; refactor `commit` to use it; add `reset_staged` on both `MempoolState` and `Mempool` |
| `crates/apollo_mempool_types/src/communication.rs` | Add `ResetStaged` variant to enums, trait method, client impl, priority |
| `crates/apollo_mempool/src/communication.rs` | Add `ResetStaged` handler in `handle_request` |
| `crates/apollo_batcher/src/batcher.rs` | Call `reset_staged()` instead of `commit_block(default())` in `propose_block` |

---

## PR 1 — Business logic (`apollo_mempool`)

### Task 1: Extract `MempoolState::drain_staged` and refactor `commit`

**File:** `crates/apollo_mempool/src/mempool.rs`

`MempoolState::commit` (lines ~132–158) currently inlines the "collect staged addresses to rewind, clear staged" logic. Extract it into a private helper so both `commit` and the new `reset_staged` share it.

The existing `commit` computes addresses-to-rewind as staged keys *not* in `address_to_nonce` (i.e. those that didn't make it into the block). Passing an empty map gives all staged keys — which is exactly what `reset_staged` needs.

**Step 1: Add `drain_staged`**

Add this private method to `MempoolState`, immediately before `commit`:

```rust
/// Returns the staged addresses not present in `committed_in_block` and clears all staged
/// nonces. Used by both `commit` and `reset_staged`.
fn drain_staged(&mut self, committed_in_block: &AddressToNonce) -> Vec<ContractAddress> {
    let addresses_to_rewind: Vec<_> = self
        .staged
        .keys()
        .filter(|&key| !committed_in_block.contains_key(key))
        .copied()
        .collect();
    self.staged.clear();
    addresses_to_rewind
}
```

**Step 2: Refactor `commit` to use `drain_staged`**

Replace the inline staged-collection and `staged.clear()` in `commit` with a call to `drain_staged`. The diff should be a net reduction. The history push and committed-nonce cleanup remain unchanged. After refactoring, `commit` should look like:

```rust
fn commit(&mut self, address_to_nonce: AddressToNonce) -> Vec<ContractAddress> {
    let addresses_to_rewind = self.drain_staged(&address_to_nonce);

    self.committed.extend(address_to_nonce.clone());

    let removed_commit = self.commit_history.push(address_to_nonce);
    for (address, removed_nonce) in removed_commit {
        let last_committed_nonce = *self
            .committed
            .get(&address)
            .expect("Account in commit history must appear in the committed nonces.");
        if last_committed_nonce == removed_nonce {
            self.committed.remove(&address);
        }
    }

    addresses_to_rewind
}
```

**Step 3: Build and test**

```bash
cargo build -p apollo_mempool 2>&1
SEED=0 cargo test -p apollo_mempool 2>&1
```

Expected: clean build, all existing tests pass (pure refactor — no behaviour change).

---

### Task 2: Add `MempoolState::reset_staged` with test

**File:** `crates/apollo_mempool/src/mempool.rs`

**Step 1: Write the failing test**

`MempoolState` is private, so the test lives in the `#[cfg(test)]` module within `mempool.rs`. Find the test module with `mod tests` in that file and add:

```rust
#[test]
fn test_reset_staged_does_not_consume_history_slot() {
    // capacity=1: the single history slot holds the most recent real commit.
    let mut state = MempoolState::new(1);

    let addr_a = contract_address!("0x1");
    let nonce_1 = nonce!(1);
    let nonce_2 = nonce!(2);

    // Real commit: addr_a nonce 1. The single history slot now holds {addr_a: nonce_1}.
    state.commit(HashMap::from([(addr_a, nonce_1)]));
    assert_eq!(state.committed.get(&addr_a), Some(&nonce_1));

    // Simulate proposal: stage addr_a at nonce 2.
    state.staged.insert(addr_a, nonce_2);

    // reset_staged must NOT push to commit_history.
    let rewound = state.reset_staged();

    assert!(state.staged.is_empty(), "staged should be cleared");
    assert_eq!(rewound, vec![addr_a], "staged addr_a should be returned for rewinding");
    assert_eq!(
        state.committed.get(&addr_a),
        Some(&nonce_1),
        "committed unchanged — no history slot consumed"
    );

    // Second real commit with capacity=1 evicts the first entry.
    // If reset_staged had consumed a slot, addr_a would already be evicted; it is not.
    let addr_b = contract_address!("0x2");
    state.commit(HashMap::from([(addr_b, nonce_1)]));

    assert_eq!(state.committed.get(&addr_a), None, "addr_a evicted by second real commit");
    assert_eq!(state.committed.get(&addr_b), Some(&nonce_1));
}
```

**Step 2: Run test to verify it fails**

```bash
SEED=0 cargo test -p apollo_mempool test_reset_staged_does_not_consume_history_slot 2>&1
```

Expected: compile error — `reset_staged` does not exist yet.

**Step 3: Implement `MempoolState::reset_staged`**

Add after `commit` (~line 158):

```rust
/// Clears staged nonces for a new proposal and returns all staged addresses for queue
/// rewinding. Does NOT advance the commit history ring buffer.
fn reset_staged(&mut self) -> Vec<ContractAddress> {
    self.drain_staged(&AddressToNonce::new())
}
```

**Step 4: Run test to verify it passes**

```bash
SEED=0 cargo test -p apollo_mempool test_reset_staged_does_not_consume_history_slot 2>&1
```

Expected: PASS.

---

### Task 3: Add `Mempool::reset_staged` (public) with test

**File:** `crates/apollo_mempool/src/mempool.rs`

The public `reset_staged` mirrors what `commit_block(CommitBlockArgs::default())` did at the `Mempool` level: rewind all staged addresses in the queue and update metrics. Examine `commit_block` (lines 603–662): after `state.commit(...)` it calls `self.rewind_txs(addresses_to_rewind, &address_to_nonce, &rejected_tx_hashes)`, then `self.update_state_metrics()`, then `self.update_accounts_with_gap(account_nonce_updates)`. With empty maps the last call is a no-op; verify this in the code and include or omit accordingly.

**Step 1: Write the failing test**

Add to the main mempool test file (follow the naming convention of nearby tests, e.g. `fee_mempool_test.rs` or `mempool_flow_tests.rs`):

```rust
#[rstest]
fn test_reset_staged_rewinds_queue() {
    // Setup: one transaction in pool and priority queue.
    let tx = tx!(tx_hash: 0, address: "0x0", tx_nonce: 0);
    let tx_ref = TransactionReference::new(&tx);
    let mut mempool = MempoolTestContentBuilder::new()
        .with_pool([tx.clone()])
        .with_priority_queue([tx_ref])
        .build_full_mempool();

    // Stage the transaction via get_txs.
    get_txs(&mut mempool, 1);

    // After staging, the transaction is no longer in the queue.
    // reset_staged must rewind it back.
    mempool.reset_staged();

    let expected = MempoolTestContentBuilder::new()
        .with_pool([tx.clone()])
        .with_priority_queue([tx_ref])
        .build();
    expected.assert_eq(&mempool.content());
}
```

**Step 2: Run test to verify it fails**

```bash
SEED=0 cargo test -p apollo_mempool test_reset_staged_rewinds_queue 2>&1
```

Expected: compile error — `reset_staged` not found on `Mempool`.

**Step 3: Implement `Mempool::reset_staged`**

Add near `commit_block` in the `impl Mempool` block:

```rust
/// Resets staged transaction state for a new proposal without advancing commit history.
/// Equivalent to `commit_block(CommitBlockArgs::default())` but does not consume a
/// `CommitHistory` slot.
pub fn reset_staged(&mut self) {
    let addresses_to_rewind = self.state.reset_staged();
    self.rewind_txs(addresses_to_rewind, &AddressToNonce::new(), &IndexSet::new());
    self.update_state_metrics();
}
```

**Step 4: Run all mempool tests**

```bash
SEED=0 cargo test -p apollo_mempool 2>&1
```

Expected: all pass.

---

## PR 2 — Interface (`apollo_mempool_types` + `apollo_mempool`)

### Task 4: Add `reset_staged` to the communication layer

**Files:**
- `crates/apollo_mempool_types/src/communication.rs`
- `crates/apollo_mempool/src/communication.rs`

**Step 1: Extend the types**

In `apollo_mempool_types/src/communication.rs`:

a. Add to `MempoolRequest` enum after `CommitBlock`:
```rust
ResetStaged,
```

b. Add to `MempoolResponse` enum after `CommitBlock`:
```rust
ResetStaged(MempoolResult<()>),
```

c. Join the `High` priority arm:
```rust
MempoolRequest::CommitBlock(_) | MempoolRequest::GetTransactions(_) | MempoolRequest::ResetStaged => {
    RequestPriority::High
}
```

d. Add to the `MempoolClient` trait after `commit_block`:
```rust
async fn reset_staged(&self) -> MempoolClientResult<()>;
```

e. Add to the generic `ComponentClient<MempoolRequest, MempoolResponse>` impl after `commit_block`:
```rust
async fn reset_staged(&self) -> MempoolClientResult<()> {
    let request = MempoolRequest::ResetStaged;
    handle_all_response_variants!(
        self,
        request,
        MempoolResponse,
        ResetStaged,
        MempoolClientError,
        MempoolError,
        Direct
    )
}
```

**Step 2: Add the server-side handler**

In `crates/apollo_mempool/src/communication.rs`, inside the `handle_request` match block after the `CommitBlock` arm (~line 242):

```rust
MempoolRequest::ResetStaged => {
    MempoolResponse::ResetStaged(Ok(self.reset_staged()))
}
```

**Step 3: Build and test**

```bash
cargo build -p apollo_mempool -p apollo_mempool_types 2>&1
SEED=0 cargo test -p apollo_mempool -p apollo_mempool_types 2>&1
```

Expected: clean build (mockall auto-generates the mock impl for the new trait method), all tests pass.

---

## PR 3 — Batcher usage (`apollo_batcher`)

### Task 5: Replace `commit_block(default)` with `reset_staged` in `propose_block`

**File:** `crates/apollo_batcher/src/batcher.rs`

**Step 1: Update `propose_block`**

Around line 307, replace:
```rust
mempool_client.commit_block(CommitBlockArgs::default()).await.map_err(|err| {
    error!(
        "Mempool is not ready to start proposal {}: {}.",
        propose_block_input.proposal_id, err
    );
    BatcherError::NotReady
})?;
```

With:
```rust
mempool_client.reset_staged().await.map_err(|err| {
    error!(
        "Mempool is not ready to start proposal {}: {}.",
        propose_block_input.proposal_id, err
    );
    BatcherError::NotReady
})?;
```

`CommitBlockArgs` is still used in `commit_proposal_and_block`, so keep its import.

**Step 2: Update mock expectations in batcher tests**

Grep for `commit_block` usages in the batcher test files:

```bash
grep -rn "commit_block" crates/apollo_batcher/src/ 2>&1
```

For every mock expectation corresponding to the `propose_block` call (the ones with `CommitBlockArgs::default()`), replace:
```rust
mock_mempool.expect_commit_block()
    .with(eq(CommitBlockArgs::default()))
    .returning(|_| Ok(()));
```
with:
```rust
mock_mempool.expect_reset_staged()
    .returning(|| Ok(()));
```

Keep any `expect_commit_block` expectations that use non-default args (those correspond to `commit_proposal_and_block`).

**Step 3: Build and test**

```bash
cargo build -p apollo_batcher 2>&1
SEED=0 cargo test -p apollo_batcher 2>&1
```

Expected: clean build, all tests pass.

**Step 4: Final check across all three crates**

```bash
SEED=0 cargo test -p apollo_mempool -p apollo_mempool_types -p apollo_batcher 2>&1
cargo clippy -p apollo_mempool -p apollo_mempool_types -p apollo_batcher 2>&1
unset CI && scripts/rust_fmt.sh
```
