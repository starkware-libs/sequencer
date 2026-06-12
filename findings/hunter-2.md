# Bug Hunter 2 Findings

## Files Examined

- `crates/apollo_mempool/src/mempool.rs` — core Mempool struct: add_tx, get_txs, commit_block, eviction, gap tracking
- `crates/apollo_mempool/src/transaction_pool.rs` — TransactionPool, AccountTransactionIndex, TimedTransactionMap
- `crates/apollo_mempool/src/fee_transaction_queue.rs` — FeeTransactionQueue with priority/pending queues
- `crates/apollo_mempool/src/fifo_transaction_queue.rs` — FifoTransactionQueue for Echonet mode
- `crates/apollo_mempool/src/utils.rs` — try_increment_nonce
- `crates/apollo_mempool/src/mempool_flow_tests.rs` — integration flow tests
- `crates/apollo_mempool/src/fee_mempool_test.rs` — unit tests for fee mode mempool
- `crates/apollo_mempool/src/transaction_pool_test.rs` — unit tests for TransactionPool

---

## Bug 1

**File**: `crates/apollo_mempool/src/mempool.rs`
**Location**: `fn handle_capacity_overflow`, line ~1011

**Description**: When the mempool is full and a transaction would close a nonce gap, the capacity overflow handler uses the **user-submitted** `account_nonce` (not the mempool's internally resolved nonce) to determine whether the transaction closes a gap. If the user submitted a stale `account_nonce` that is lower than the mempool's resolved nonce but the `tx_nonce` still correctly equals the mempool's expected next nonce, the handler incorrectly classifies the transaction as "creating a gap" and returns `MempoolFull` without attempting eviction.

**Root Cause**:

```rust
fn handle_capacity_overflow(
    &mut self,
    tx: &InternalRpcTransaction,
    account_nonce: Nonce,   // <-- raw user-submitted nonce, NOT mempool-resolved
) -> Result<(), MempoolError> {
    let address = tx.contract_address();

    let account_has_gap = self.accounts_with_gap.contains(&address);
    let account_has_txs = self.tx_pool.contains_account(address);
    let closing_gap = tx.nonce() == account_nonce;   // <-- BUG: compares against raw nonce
    let creating_gap = (account_has_gap || !account_has_txs) && !closing_gap;

    if !creating_gap && self.try_make_space(tx.total_bytes()) {
        return Ok(());
    }

    Err(MempoolError::MempoolFull)
}
```

The `closing_gap` check should compare against the mempool's internally resolved nonce:
```rust
let mempool_account_nonce = self.state.resolve_nonce(address, account_nonce);
let closing_gap = tx.nonce() == mempool_account_nonce;  // correct
```

**Scenario to reproduce**:
1. Account 0x0 submits tx at nonce 0 with account_nonce 0. It is committed (committed["0x0"] = 1).
2. Account 0x0 submits tx at nonce 2 with account_nonce 0 (stale but accepted since nonce 2 >= resolved 1). This creates a gap (pool has nonce 2, but resolved nonce is 1).
3. Mempool fills to capacity with another account's (evictable) tx.
4. Account 0x0 submits tx at nonce 1 with account_nonce 0 (stale — user hasn't seen the commit yet). This tx **should close the gap** (nonce 1 == mempool-resolved nonce 1), which should trigger eviction of the gap account to free space.
5. BUG: `closing_gap = (1 == 0) = false`. `account_has_gap = true`. `creating_gap = true`. Returns `MempoolFull` without attempting eviction, even though this tx would close the existing gap.

Note: The transaction passes nonce validation (in `validate_incoming_tx`, `resolve_nonce(0x0, 0) = 1`, and `tx_nonce = 1 >= 1` is valid), so the stale submitted nonce is not rejected earlier.

**Failing Test**:
```rust
#[test]
fn test_gap_closing_tx_with_stale_account_nonce_rejected_incorrectly() {
    use std::sync::Arc;
    use apollo_mempool_config::config::{MempoolConfig, MempoolStaticConfig};
    use apollo_mempool_types::errors::MempoolError;
    use apollo_time::test_utils::FakeClock;
    use crate::mempool::Mempool;
    use crate::test_utils::{add_tx, add_tx_expect_error, commit_block, get_txs_and_assert_expected};
    use crate::{add_tx_input};

    // Step 1: Add nonce 0 and commit it so mempool has committed["0x0"] = 1.
    let nonce0_tx = add_tx_input!(tx_hash: 1, address: "0x0", tx_nonce: 0, account_nonce: 0);
    // tx that will create the gap (nonce 2 submitted, but resolved nonce is 1)
    let nonce2_tx = add_tx_input!(tx_hash: 2, address: "0x0", tx_nonce: 2, account_nonce: 0);
    // An evictable gap-account tx to occupy capacity
    let evictable_tx = add_tx_input!(tx_hash: 3, address: "0x1", tx_nonce: 1, account_nonce: 0);

    // Size the mempool to hold exactly two transactions.
    let capacity = nonce2_tx.tx.total_bytes() + evictable_tx.tx.total_bytes();
    let mut mempool = Mempool::new(
        MempoolConfig {
            static_config: MempoolStaticConfig {
                capacity_in_bytes: capacity,
                ..Default::default()
            },
            ..Default::default()
        },
        Arc::new(FakeClock::default()),
    );

    // Add and commit nonce 0 so mempool's resolved nonce for 0x0 advances to 1.
    add_tx(&mut mempool, &nonce0_tx);
    get_txs_and_assert_expected(&mut mempool, 1, &[nonce0_tx.tx]);
    commit_block(&mut mempool, [("0x0", 1)], []);

    // Add nonce 2 (with stale account_nonce=0; passes validation because resolve_nonce(0x0,0)=1
    // and nonce 2 >= 1). This creates a gap: pool has nonce 2, mempool expects nonce 1.
    add_tx(&mut mempool, &nonce2_tx);

    // Fill remaining capacity with an evictable tx (gap account for 0x1).
    add_tx(&mut mempool, &evictable_tx);

    // Now submit nonce 1 with STALE account_nonce=0. The mempool's resolved nonce for 0x0 is 1,
    // so this tx fills the gap. Evicting 0x0's nonce-2 tx should free exactly enough space.
    // BUG: closing_gap = (1 == 0) = false => creating_gap = true => MempoolFull returned.
    // CORRECT: the tx should be accepted because it closes the gap and eviction should proceed.
    let gap_closing_tx = add_tx_input!(tx_hash: 4, address: "0x0", tx_nonce: 1, account_nonce: 0);
    // This assertion fails with the bug: returns MempoolFull instead of Ok.
    add_tx(&mut mempool, &gap_closing_tx);
}
```

**How to Verify**: `cargo test -p apollo_mempool test_gap_closing_tx_with_stale_account_nonce_rejected_incorrectly`

The test will **fail** (panics with "Expected Ok, got Err(MempoolFull)") because `handle_capacity_overflow` uses the raw `account_nonce = 0` instead of the mempool-resolved `account_nonce = 1`, causing `closing_gap = false` and `creating_gap = true`, which immediately returns `MempoolFull` without attempting to evict the gap account.

---

## Additional Areas Reviewed Without Confirmed Bugs

- **`remove_up_to_nonce` boundary**: `BTreeMap::split_off(&nonce)` correctly removes transactions with nonce strictly less than `nonce` (committed next-nonce), preserving the committed-nonce tx in the pool. Semantics match the caller's expectation.

- **`promote_txs_to_priority` / `demote_txs_to_pending`**: The `GasPrice` comparisons are consistent with the `insert` logic: priority queue holds `max_l2_gas_price >= threshold`, pending holds `< threshold`. The `split_off` boundary uses `>=` which is correct.

- **`SubmissionID` ordering in `remove_txs_older_than`**: The reversed timestamp ordering (older = greater) is consistent; `split_off` returns older-than-cutoff transactions correctly.

- **`n_stuck_txs` counter**: Increments and decrements track correctly across `add_tx_inner`, `remove_from_accounts_with_gap`, `decrement_stuck_txs_if_gap_account`, and `try_make_space`. No underflow scenario identified.

- **Fee escalation boundary (`increased_enough`)**: Uses `checked_mul` and `checked_add` for overflow protection. Overflow → returns false (rejects), which is safe.

- **FIFO rewind logic**: Complex but the staging and rewind lifecycle appears internally consistent for the tested scenarios, though the behavior when a tx at the committed nonce is rejected (and higher nonces were also staged) may leave higher-nonce txs unreachable — this edge case is harder to classify definitively without the full FIFO spec.

- **Rejected tx followed by pool siblings (fee mode)**: When a rejected tx is removed via `remove_rejected_txs`, `remove_by_address` is called, which could in theory remove a freshly-re-enqueued sibling. Analysis shows the scenarios in which this could be wrong collapse either to already-correct behavior (nothing to queue), or to cases where the sibling is logically blocked by the missing committed nonce (correct behavior).
