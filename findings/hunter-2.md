# Bug Hunter #2 Findings — apollo_batcher

Crate audited: `apollo_batcher` at `/home/user/sequencer/crates/apollo_batcher/src/`

Files read in depth:
- `batcher.rs` (2012 lines)
- `block_builder.rs` (949 lines)
- `transaction_provider.rs` (237 lines)
- `utils.rs` (83 lines)
- `pre_confirmed_block_writer.rs` (236 lines)
- `commitment_manager/commitment_manager_impl.rs` (567 lines)
- `commitment_manager/state_committer.rs` (154 lines)
- `commitment_manager/types.rs` (183 lines)
- `batcher_test.rs` (partial)
- `transaction_provider_test.rs` (full)
- `block_builder_test.rs` (partial)

---

## Bug 1: ValidateTransactionProvider::get_txs silently drops transactions adjacent to an invalid L1Handler

**File**: `/home/user/sequencer/crates/apollo_batcher/src/transaction_provider.rs`, lines 187–224

**Description**:
`ValidateTransactionProvider::get_txs` dequeues up to `n_txs` messages from `tx_receiver` in a single `recv_many` call, then validates any L1Handler transactions in the batch. When an invalid L1Handler is found, the function returns an error immediately — but all other transactions that were already dequeued into `buffer` (both before and after the invalid L1Handler in the buffer) are silently discarded when `buffer` drops.

Specifically:
- Transactions **before** the invalid L1Handler in the buffer have been dequeued and are now gone from the channel. They are never returned to the caller and never executed by the validator.
- Transactions **after** the invalid L1Handler are also gone.

**Root Cause**:
`recv_many` dequeues items into a local `buffer`. If any L1Handler in `buffer` fails validation (line 214), the function returns `Err(...)` while `buffer` still contains the remaining items. When `buffer` goes out of scope, all those items are permanently lost from the channel. Future `get_txs` calls cannot retrieve them.

```rust
// transaction_provider.rs lines 195–222
let mut buffer = Vec::with_capacity(n_txs);
self.tx_receiver.recv_many(&mut buffer, n_txs).await;  // dequeues all at once

for tx in &buffer {
    if let InternalConsensusTransaction::L1Handler(tx) = tx {
        // ...validate...
        if let L1ValidationStatus::Invalid(validation_status) = l1_validation_status {
            return Err(TransactionProviderError::L1HandlerTransactionValidationFailed {
                tx_hash: tx.tx_hash,
                validation_status,
            });
            // buffer is dropped here — all items in it are permanently lost from channel
        }
        continue;
    }
}
Ok(buffer)  // only reached if no invalid L1Handler found
```

**Impact**:
Since the entire proposal is rejected when this error is returned (`FailOnError(L1HandlerTransactionValidationFailed)` → `InvalidProposal`), there is no state corruption. However, the diagnostic state is incorrect: the proposer's block builder has executed transactions that the validator never received, creating a phantom divergence. More importantly, the lost transactions were permanently removed from the channel — any attempt to diagnose the validator's state would show them as neither executed nor rejected.

**Test**:
```rust
// In crates/apollo_batcher/src/transaction_provider_test.rs
// Run with: SEED=0 cargo test -p apollo_batcher drop_buffered_txs_on_l1handler_failure

#[rstest]
#[tokio::test]
async fn drop_buffered_txs_on_l1handler_failure(
    mut mock_dependencies: MockDependencies,
) {
    let test_tx = test_l1handler_tx();

    // L1Handler is invalid.
    mock_dependencies.expect_validate_l1handler(
        test_tx.clone(),
        L1ValidationStatus::Invalid(InvalidValidationStatus::NotFound),
    );

    // Send: [invoke_before, l1_handler(invalid), invoke_after]
    // All 3 are in the channel before get_txs is called.
    let invoke_before = InternalConsensusTransaction::RpcTransaction(
        internal_invoke_tx(InvokeTxArgs::default()),
    );
    let invoke_after = InternalConsensusTransaction::RpcTransaction(
        internal_invoke_tx(InvokeTxArgs::default()),
    );
    mock_dependencies
        .simulate_input_txs(vec![
            invoke_before.clone(),
            InternalConsensusTransaction::L1Handler(test_tx),
            invoke_after.clone(),
        ])
        .await;

    let mut validate_tx_provider = mock_dependencies.validate_tx_provider();

    // First call: recv_many dequeues all 3 txs, L1Handler is invalid → error returned.
    // invoke_before and invoke_after are permanently lost from the channel.
    let result = validate_tx_provider.get_txs(MAX_TXS_PER_FETCH).await;
    assert_matches!(
        result,
        Err(TransactionProviderError::L1HandlerTransactionValidationFailed { .. })
    );

    // Bug demonstrated: channel is now empty despite never having returned
    // invoke_before or invoke_after to the caller.
    // A subsequent get_txs call returns Ok([]) — the transactions have vanished.
    let result2 = validate_tx_provider.get_txs(MAX_TXS_PER_FETCH).await;
    assert_eq!(result2.unwrap(), vec![]);
    // If the bug were fixed (buffer returned before the invalid L1Handler),
    // this would return Ok([invoke_before]).
}
```

**How to verify**:
```
SEED=0 cargo test -p apollo_batcher drop_buffered_txs_on_l1handler_failure
```

---

## Bug 2: `send_txs_for_proposal` panics instead of returning a recoverable error

**File**: `/home/user/sequencer/crates/apollo_batcher/src/batcher.rs`, line 603

**Description**:
In `send_txs_for_proposal`, when the proposal is no longer active and its result is `Ok(_)`, the code panics:

```rust
Ok(_) => panic!("Proposal finished validation before all transactions were sent."),
```

This path is described as "should never happen in practice", but it represents a case where the block builder completed successfully BEFORE the proposer finished sending transactions. If this condition ever occurs — due to a protocol bug, a misbehaving consensus layer, or an unforeseen timing issue — the entire batcher process crashes rather than returning a recoverable `BatcherError`.

**Root Cause**:
The validator's block builder exits successfully only when `n_executed_txs >= final_n_executed_txs`, and `final_n_executed_txs` is sent only by `await_active_proposal` which is called by `finish_proposal`. In normal operation, `finish_proposal` is always called AFTER all transactions are sent. However, using `panic!` for this condition is inappropriate because:
1. It crashes the batcher process — a non-recoverable failure
2. The condition is reachable in theory (e.g., consensus sends `finish_proposal` prematurely, or a future refactor changes the ordering)
3. The correct response is `Err(BatcherError::InternalError)` or a new specific error variant

**Test (justification — mechanical reproduction is protocol-ordering dependent)**:
```rust
// The panic at batcher.rs:603 is triggered by this code path in send_txs_for_proposal:
//
// 1. validate_block() is called — spawns block builder task
// 2. Block builder task finishes (e.g., reaches deadline without executing txs, returns Ok)
// 3. send_txs_for_proposal() is called AFTER the task finishes
// 4. is_active() returns false (task has completed)
// 5. get_completed_proposal_result() returns Ok(_)
// 6. panic!("Proposal finished validation before all transactions were sent.")
//
// The fix is simple: replace the panic with:
//   Err(BatcherError::InternalError)
// at batcher.rs line 603.
//
// This is a code quality / defensive programming bug. The severity is: any unexpected
// ordering that causes this path to be taken crashes the batcher process instead of
// returning an error that consensus can handle.
```

**How to verify**: Read `batcher.rs` line 600–613. The `panic!` at line 603 is in a `match` arm for `Ok(_)` in a code path that, while unlikely in correct operation, should return a graceful error instead of crashing the process.

---

## Bug 3: `proposals_counter` starts at 1 and is not reset on `start_height`, causing L1 phase cadence to desync across heights

**File**: `/home/user/sequencer/crates/apollo_batcher/src/batcher.rs`, lines 282–283 and 395–401

**Description**:
`proposals_counter` is initialized to 1 and is never reset between heights (only cleared in `new()`). The L1 transaction phase is determined by:

```rust
let start_phase = if self.proposals_counter.is_multiple_of(self.config.static_config.propose_l1_txs_every) {
    TxProviderPhase::L1
} else {
    TxProviderPhase::Mempool
};
// ... (later)
self.proposals_counter += 1;
```

This means the L1 phase cadence is based on the **total number of proposals ever made**, not the number of proposals at the current height. The comment says "Allow the first few proposals to be without L1 txs while system starts up." This is an initialization comment, but the counter's behavior has a subtle design issue: the cadence of L1 proposals accumulates across all heights and is never reset.

**Concrete scenario with `propose_l1_txs_every = 3`**:
- Height 10: proposals 1, 2, 3 → L1 on proposal 3
- Height 11: proposals 4, 5, 6 → L1 on proposal 6
- After 100 proposals over many heights, counter is 101; L1 fires at 102, 105, 108...
- After a batcher restart (counter resets to 1), L1 fires at 3, 6, 9...

This is **inconsistent with the test expectation** at `batcher_test.rs` line 973, which tests a fixed N_PROPOSALS = 4 (heights reused via `abort_active_height`). The test verifies L1 fires every 3rd CALL to `propose_block`, regardless of height. This is the intended behavior.

However, the lack of a reset means the cadence depends on historical state, not per-height semantics. If the caller expects "L1 every 3rd proposal PER HEIGHT", the behavior is wrong. This is a documentation/design clarity bug — the counter should be named `total_proposals_counter` and its cross-height semantics should be documented.

**Assessment**: This is a design-level ambiguity rather than a correctness bug given the current test coverage. Not marking as critical but worth documenting.

---

## Bug 4 (Design): `get_proposal_content` returns `InternalError` on proposal failure instead of a meaningful error

**File**: `/home/user/sequencer/crates/apollo_batcher/src/batcher.rs`, lines 800–815

**Description**:
When `get_proposal_content` is called on a proposal that has completed with an error, it returns `BatcherError::InternalError`:

```rust
let finished_proposal_info = self
    .get_completed_proposal_result(proposal_id)
    .await?
    .expect("Proposal should exist.")
    .map_err(|err| {
        error!("Failed to get commitment: {}", err);
        BatcherError::InternalError   // ← all proposal errors become InternalError
    })?;
```

A proposal can fail for various reasons (deadline reached, block full, transaction failed, aborted). When called via `get_proposal_content`, ALL failure modes are mapped to the same opaque `InternalError`. The caller cannot distinguish between a recoverable error (aborted, deadline) and an internal batcher failure.

This contrasts with `finish_proposal` which properly distinguishes these cases via `proposal_status_from`.

**Root Cause**:
The `get_proposal_content` path only handles the success case (streaming transactions). When the proposer's block builder fails, the expected flow is: `get_proposal_content` returns some transactions and then `Finished`. But if the block builder fails before producing any transactions, `get_proposal_content` sees `0 transactions`, tries to get the commitment, finds an error, and returns `InternalError`.

The correct fix is to apply `proposal_status_from` in `get_proposal_content` similarly to `finish_proposal`, returning appropriate errors.

**How to verify**: `batcher.rs` lines 789–815. Trace what happens when the block builder stored an `Err` result: `get_completed_proposal_result` returns `Some(Err(...))`, the `.map_err` converts it to `BatcherError::InternalError`.

---

## Summary of genuine bugs found

1. **Bug 1 (Confirmed)**: `ValidateTransactionProvider::get_txs` at `transaction_provider.rs:214` drops all transactions buffered by `recv_many` when an invalid L1Handler is found mid-batch. The dequeued-but-unprocessed transactions are permanently lost from the channel. This does not cause state corruption (the proposal fails anyway), but it creates a gap between what the proposer considers "sent" and what the validator received.

2. **Bug 2 (Confirmed, low-severity)**: `batcher.rs:603` uses `panic!()` where a recoverable error should be returned. A process crash is unacceptable for a condition that could be triggered by protocol-layer misbehavior.

3. **Bug 4 (Design bug, confirmed)**: `batcher.rs:806` maps all proposal errors to `BatcherError::InternalError` in the `get_proposal_content` path, losing diagnostic information that `finish_proposal` properly preserves.

4. **Bug 3 (Design ambiguity)**: `proposals_counter` is never reset between heights, making the L1 cadence dependent on total historical proposals rather than per-height semantics.
