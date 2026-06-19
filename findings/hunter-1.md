# Bug Hunting Report — apollo_mempool

**Crate**: `crates/apollo_mempool/src/`  
**Hunter**: #1

---

## Bug 1: Rejected next-eligible transaction leaves successor permanently stuck in pool (fee-priority mode)

**File**: `crates/apollo_mempool/src/mempool.rs`, lines 548–583 (`remove_rejected_txs`) and lines 663–676 (`commit_block`)

**Description**

In fee-priority mode, when the next-eligible transaction for an account is both committed (its nonce advances the account) AND the transaction at the new account nonce is rejected in the same `commit_block` call, the transaction at `account_nonce + 1` is left permanently in the pool without being inserted into the priority queue. The tx is stuck: it will never be returned by `get_txs` unless a new `add_tx` for that account arrives.

**Root Cause**

The `commit_block` flow is:

1. For each `(address, next_nonce)` in `address_to_nonce`:
   - Remove pool txs with nonce `< next_nonce` (committed).
   - If queue is empty for this address, insert the tx at `next_nonce` into the queue (the gap-fill at line 670–676).
2. Call `remove_rejected_txs`, which for each rejected tx:
   - Removes it from the pool.
   - Calls `tx_queue.remove_by_address(address)` — removing WHATEVER is currently in the queue for that address (line 567).

Concretely: if nonce 4 was committed (`next_nonce=5`), nonce 5 is rejected, and nonce 6 is in the pool:
- Step 1 inserts nonce 5 into the queue (gap-fill).
- Step 2 removes nonce 5 from pool and queue (correct).
- Nonce 6 is never considered for queue insertion. The account's committed nonce is 5, the pool holds nonce 6, but the queue has no entry for this address.

`update_accounts_with_gap` marks the account as having a gap (account_nonce=5 < lowest pool nonce=6), but this function only updates eviction tracking — it does not re-enqueue the next eligible tx.

**Test**

```rust
// This test must be run inside the apollo_mempool crate, e.g. in fee_mempool_test.rs
// It requires the existing test infrastructure (add_tx_input!, add_tx, commit_block,
// get_txs_and_assert_expected helpers) already present in the test module.

#[rstest]
fn test_rejection_of_exact_next_nonce_leaves_successor_stuck(mut mempool: Mempool) {
    // Setup: account 0x0 has 3 sequential txs starting at nonce 0.
    let tx_nonce_0 = add_tx_input!(tx_hash: 1, address: "0x0", tx_nonce: 0, account_nonce: 0, tip: 10);
    let tx_nonce_1 = add_tx_input!(tx_hash: 2, address: "0x0", tx_nonce: 1, account_nonce: 0, tip: 10);
    let tx_nonce_2 = add_tx_input!(tx_hash: 3, address: "0x0", tx_nonce: 2, account_nonce: 0, tip: 10);

    for input in [&tx_nonce_0, &tx_nonce_1, &tx_nonce_2] {
        add_tx(&mut mempool, input);
    }

    // Stage nonce 0 and 1 (get_txs enqueues nonce 1 after returning nonce 0, then pops nonce 1).
    get_txs_and_assert_expected(&mut mempool, 2, &[tx_nonce_0.tx, tx_nonce_1.tx.clone()]);

    // Block commits nonce 0 (next_nonce = 1); nonce 1 tx is rejected.
    // Nonce 2 is in pool and should be the next eligible tx after the commit.
    commit_block(
        &mut mempool,
        [("0x0", 1)], // Account 0x0 advanced to nonce 1.
        [tx_nonce_1.tx.tx_hash], // Nonce 1 was rejected.
    );

    // BUG: nonce 2 is in the pool but was never added to the queue.
    // Expected: get_txs returns nonce 2 (it is the next eligible tx).
    // Actual:   get_txs returns nothing, tx is stuck.
    get_txs_and_assert_expected(&mut mempool, 1, &[tx_nonce_2.tx]);
}
```

**How to verify**

```bash
cd /home/user/sequencer
SEED=0 cargo test -p apollo_mempool test_rejection_of_exact_next_nonce_leaves_successor_stuck
```

The test will fail because `get_txs` returns an empty vec instead of `[tx_nonce_2.tx]`, demonstrating that nonce 2 is permanently stuck.

---

## Bug 2: Panic when `committed_nonce_retention_block_count` is configured to zero

**File**: `crates/apollo_mempool/src/mempool.rs`, lines 71–79 (`CommitHistory::push`)

**Description**

If a user configures `committed_nonce_retention_block_count = 0`, calling `commit_block` panics immediately with `"Commit history should be initialized with capacity."`.

**Root Cause**

`CommitHistory::new(0)` creates an empty `VecDeque` (zero elements). `CommitHistory::push` calls `pop_front()` on this empty deque, which returns `None`, and then calls `.expect(...)` which panics.

```rust
fn push(&mut self, commit: AddressToNonce) -> AddressToNonce {
    let removed_commit = self.commits.pop_front();   // None when capacity=0
    self.commits.push_back(commit);
    removed_commit.expect("Commit history should be initialized with capacity.") // PANIC
}
```

There is no validation of `committed_nonce_retention_block_count` anywhere (a `TODO` comment at the config level notes "should be bounded?"). An operator setting the value to 0 to disable the retention feature — a reasonable interpretation of the name — causes a crash on the first `commit_block` call.

**Test**

```rust
// Can be added anywhere in the fee_mempool_test.rs file.
#[test]
fn test_commit_block_panics_with_zero_retention_count() {
    use std::sync::Arc;
    use apollo_mempool_config::config::{MempoolConfig, MempoolStaticConfig};
    use apollo_time::test_utils::FakeClock;
    use crate::mempool::Mempool;
    use crate::test_utils::commit_block;

    let mut mempool = Mempool::new(
        MempoolConfig {
            static_config: MempoolStaticConfig {
                committed_nonce_retention_block_count: 0,
                ..Default::default()
            },
            ..Default::default()
        },
        Arc::new(FakeClock::default()),
    );

    // This will panic instead of committing an empty block.
    commit_block(&mut mempool, [], []);
}
```

**How to verify**

```bash
cd /home/user/sequencer
SEED=0 cargo test -p apollo_mempool test_commit_block_panics_with_zero_retention_count -- --should-panic
```

Or run without `should-panic` to observe the panic message:

```bash
SEED=0 cargo test -p apollo_mempool test_commit_block_panics_with_zero_retention_count 2>&1 | grep -A5 "panicked"
```

---

## Bug 3: `remove_rejected_txs` calls `remove_by_address` unconditionally, potentially evicting a different queued transaction

**File**: `crates/apollo_mempool/src/mempool.rs`, lines 559–576 (`remove_rejected_txs`)

**Description**

For every rejected transaction, `remove_rejected_txs` calls `self.tx_queue.remove_by_address(address)`. In fee-priority mode, `remove_by_address` removes **whatever is currently queued** for that address — not necessarily the rejected transaction itself. If the rejected tx is not the one currently in the queue (e.g., the queued tx was already removed by an earlier step in `commit_block`, and a different tx was inserted), removing by address removes the wrong queued tx.

**Root Cause**

The code pattern is:

```rust
if let Ok(tx) = self.tx_pool.remove(tx_hash) {
    self.tx_queue.remove_by_address(tx.contract_address()); // removes WHATEVER is queued
    ...
}
```

During `commit_block`, before `remove_rejected_txs` is called, the gap-filling code at lines 670–676 inserts `tx_at_next_nonce` into the queue. If `tx_at_next_nonce` is in the rejected set, `remove_rejected_txs` correctly removes it. However, if a *subsequent* tx (say nonce=N+1) for the same address was already in the queue before this commit (e.g., re-queued from a previous cycle), calling `remove_by_address` removes nonce N+1 instead. The account then has no queue entry even though it should.

This is the underlying structural issue behind Bug 1: the fix for Bug 1 (adding nonce+1 to the queue after nonce N is rejected) would also be incorrect if `remove_by_address` blindly clears the queue without verifying it's removing exactly the rejected tx.

**Written Justification** (test not provided because the exact scenario requires careful setup of prior-queue state)

The fix should change `remove_rejected_txs` to use a targeted removal: check whether the currently-queued tx for the address matches the rejected tx hash before calling `remove_by_address`. If the queued tx is different from the rejected one (i.e., the rejected tx was not in the queue), no queue removal should occur. Example fix:

```rust
// Instead of:
self.tx_queue.remove_by_address(tx.contract_address());

// Should be:
let queued_nonce = self.tx_queue.get_nonce(tx.contract_address());
if queued_nonce == Some(tx.nonce()) {
    self.tx_queue.remove_by_address(tx.contract_address());
}
```

---

## Summary

| # | Title | Severity | File |
|---|-------|----------|------|
| 1 | Rejected next-eligible tx leaves successor permanently stuck | High | `mempool.rs:548–676` |
| 2 | Panic when `committed_nonce_retention_block_count = 0` | Medium | `mempool.rs:71–79` |
| 3 | `remove_rejected_txs` removes wrong queued tx via `remove_by_address` | Medium | `mempool.rs:566–567` |
