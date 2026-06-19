# Supervisor #1 Validation Report

**Scope**: Findings from Bug Hunters 1–4 covering `apollo_mempool`, `apollo_batcher`, `apollo_gateway`, `apollo_consensus`.

---

## Summary Table

| Bug ID | Title | Verdict | Severity |
|--------|-------|---------|----------|
| H1-B1 | Rejected next-eligible tx leaves successor stuck in queue | confirmed | High |
| H1-B2 | Panic when `committed_nonce_retention_block_count = 0` | confirmed | Medium |
| H1-B3 | `remove_rejected_txs` may evict wrong queued tx via `remove_by_address` | suspected | Medium |
| H2-B1 | `ValidateTransactionProvider::get_txs` drops buffered txs on L1Handler failure | confirmed | Low |
| H2-B2 | `send_txs_for_proposal` panics on `Ok(_)` instead of returning error | confirmed | Low |
| H2-B3 | `proposals_counter` not reset between heights | rejected | — |
| H2-B4 | `get_proposal_content` maps all errors to `InternalError` | confirmed | Low |
| H3-B1 | `P2pPropagatorClientError` returns internal error in deprecated gateway path | confirmed | Medium |
| H3-B2 | Nonce range check wraps on Felt overflow near field prime | confirmed | Low |
| H3-B3 | `max_nonce_for_validation_skip` config field has no runtime effect | confirmed | Low |
| H3-B4 | `mempool_client_result_to_gw_spec_result` is dead code | confirmed | Low |
| H3-B5 | Calldata length check conflates proof_facts, error label is misleading | confirmed | Low |
| H4-B1 | Integer overflow in `should_cache_msg` round-limit check | confirmed | Medium |
| H4-B2 | Late-duplicate stream message destroys active stream state | confirmed | High |
| H4-B3 | `inbound_send` returns `false` for Fin, conflating success with error | rejected | — |
| H4-B4 | `handle_vote_broadcasted` panics for observer SHC instances | suspected | Medium |

---

## Hunter 1 — apollo_mempool

### H1-B1: Rejected next-eligible tx leaves successor stuck in queue

**Verdict**: `confirmed`

**What I checked**:
`commit_block` at `mempool.rs:635–694` processes committed addresses first (lines 644–677), then calls `remove_rejected_txs`. The gap-filling code at lines 670–676 inserts a tx at `next_nonce` into the queue only if the queue for that address is currently empty. `remove_rejected_txs` (lines 548–583) then removes the rejected tx from the pool *and* calls `remove_by_address` on the queue unconditionally — which removes whatever was inserted by the gap-fill. After removal, the code does not attempt to re-insert the next nonce (e.g., nonce+1). The only follow-up is `update_accounts_with_gap` (line 693), which solely manages the eviction-tracking set and gap metrics; it does not enqueue any tx.

Concretely, the hunter's scenario (nonce 0 committed, nonce 1 rejected, nonce 2 in pool) maps exactly to the code. After commit_block:
- Nonce 1 is removed from pool and queue (correct).
- Nonce 2 is in the pool.
- The queue has no entry for the address.
- `update_accounts_with_gap` marks the account as having a gap (nonce 2 > committed nonce 1), but never enqueues nonce 2.
- No subsequent `add_tx` or `get_txs` call will repair this; the tx is stuck.

The test uses only normal test helpers (`add_tx`, `commit_block`, `get_txs_and_assert_expected`) — no private-field manipulation. The scenario (submit 3 txs, commit nonce 0, reject nonce 1) is a realistic operational path.

**Fix suggestion**: After `remove_rejected_txs`, for each address whose queue entry was removed, attempt to insert the tx at the committed nonce into the queue (mirror the gap-fill logic at lines 670–676 but targeted at the post-rejection state). Alternatively, refactor `remove_rejected_txs` to call the same gap-fill after each removal.

---

### H1-B2: Panic when `committed_nonce_retention_block_count = 0`

**Verdict**: `confirmed`

**What I checked**:
`CommitHistory::new(0)` at `mempool.rs:71–72` creates a `VecDeque` initialized by `repeat_n(empty_map, 0)` — an empty deque with no elements. `CommitHistory::push` at lines 75–79 calls `pop_front()` on this empty deque, which returns `None`, then immediately calls `.expect("Commit history should be initialized with capacity.")` — a guaranteed panic.

There is no validation of `committed_nonce_retention_block_count` in the config layer (a TODO comment at the config site notes "should be bounded?"). Setting it to 0 to "disable" retention is a reasonable operator intent that the code never guards against.

The test is straightforward: construct a `Mempool` with the zero-retention config and call `commit_block` — no artificial state required.

**Fix suggestion**: Add a config validation that requires `committed_nonce_retention_block_count >= 1`, or handle `capacity == 0` in `CommitHistory::push` by returning an empty `AddressToNonce` without panicking.

---

### H1-B3: `remove_rejected_txs` may evict wrong queued tx via `remove_by_address`

**Verdict**: `suspected`

**What I checked**:
The structural concern is real. `remove_rejected_txs` at line 567 calls `self.tx_queue.remove_by_address(tx.contract_address())` — this removes *whatever nonce is currently queued* for the address, not necessarily the rejected tx's nonce. The hunter correctly identifies that the gap-fill at lines 670–676 could insert a *different* tx (say nonce N+1) before `remove_rejected_txs` runs; then `remove_by_address` would remove nonce N+1 instead of the rejected nonce N tx.

However, the hunter does not provide a concrete test, and the actual scenario requires carefully staged prior-queue state that is unusual. In normal fee-priority mode:
- Txs from an address are processed serially; if nonce N is rejected, nonce N+1 was not yet in the queue (it was waiting for nonce N to execute).
- The gap-fill at line 670–676 inserts nonce `next_nonce` (the committed nonce, not next_nonce+1). If `next_nonce` is the rejected tx's nonce, the gap-fill inserts the rejected tx — and `remove_by_address` removes the correct tx.
- The problematic scenario (a *different* nonce already in the queue) requires a specific sequence that is not demonstrated.

The concern is structurally valid as an underlying design issue (Bug 1 is in part caused by it), but the claim that it removes the "wrong" tx is not demonstrated for a reachable scenario. Upgrading to confirmed requires a concrete reproducible test.

**What would make it confirmable**: A test that puts nonce N+1 in the queue (which requires it to have been the gap-fill choice from a prior commit cycle), then calls `commit_block` committing nonce N-1 with nonce N rejected — demonstrating that `remove_by_address` removes nonce N+1.

---

## Hunter 2 — apollo_batcher

### H2-B1: `ValidateTransactionProvider::get_txs` drops buffered txs on L1Handler failure

**Verdict**: `confirmed`

**What I checked**:
`ValidateTransactionProvider::get_txs` at `transaction_provider.rs:195–223` calls `recv_many` into a local `buffer`, then iterates. On finding an invalid L1Handler, it returns `Err(...)` at line 215. The `buffer` — which may contain valid invoke transactions before and after the L1Handler — goes out of scope and is dropped. These transactions are permanently dequeued from the channel and lost.

The impact claim is accurate: since the proposal is rejected anyway (the validator returns `InvalidProposal` on this error), there is no state corruption — the block is simply invalid. But the lost transactions were consumed from the channel and never returned.

The proposed test is legitimate: it constructs a channel with `[invoke_before, l1_handler(invalid), invoke_after]`, calls `get_txs`, asserts the error, then calls `get_txs` again and asserts the channel is empty. This is standard async channel testing; no private-internals access is needed. The test accurately demonstrates the bug.

**Fix suggestion**: On L1Handler validation failure, drain the remaining `buffer` back into the channel, or return the valid prefix before the L1Handler (requires buffering the rest for a subsequent call), or separate L1Handler validation from the recv step.

---

### H2-B2: `send_txs_for_proposal` panics on `Ok(_)` instead of returning error

**Verdict**: `confirmed`

**What I checked**:
`batcher.rs:603` contains `panic!("Proposal finished validation before all transactions were sent.")`. This is in the `Ok(_)` arm of a match on `get_completed_proposal_result`. The hunter correctly notes:
1. The path is reachable if the block builder task completes (`Ok`) before `send_txs_for_proposal` finishes sending — e.g., if the batcher is given a very tight deadline and the block builder exits early, or due to future refactoring.
2. A process crash (`panic!`) is not an appropriate response to a condition that could arise from protocol-layer misbehavior.

No test is provided, but the finding does not require one — reading the code at line 603 confirms the `panic!` is present where a graceful `Err(BatcherError::InternalError)` should be. This is a low-severity but real correctness issue.

**Fix suggestion**: Replace `panic!("...")` with `Err(BatcherError::InternalError)` (or a more specific error variant).

---

### H2-B3: `proposals_counter` not reset between heights

**Verdict**: `rejected`

**What I checked**:
The hunter describes this as a "design-level ambiguity rather than a correctness bug" — and then provides no test. Reading `batcher.rs:282–283` and 395–401: `proposals_counter` starts at 1 and increments per `propose_block` call. The L1 phase fires when `proposals_counter` is a multiple of `propose_l1_txs_every`. The comment says "Allow the first few proposals to be without L1 txs while system starts up." The cross-height accumulation is the intended behavior — L1 txs are included globally every N proposals, not every N proposals per height. The test at `batcher_test.rs:973` confirms this is by design.

The hunter themselves concede "This is the intended behavior" and classify it as "design-level ambiguity." There is no bug: the counter naming and comment are adequate, and the behavior is consistent with tests. This does not meet the threshold for a confirmed bug.

---

### H2-B4: `get_proposal_content` maps all errors to `BatcherError::InternalError`

**Verdict**: `confirmed`

**What I checked**:
`batcher.rs:801–808` applies `.map_err(|err| { error!(...); BatcherError::InternalError })` to the result of `get_completed_proposal_result`. When the block builder has stored an `Err`, all error variants — `InvalidProposal`, deadline, abort — are collapsed to the opaque `InternalError`. This contrasts with `finish_proposal` at lines 634–641 which correctly calls `proposal_status_from(err)?` to distinguish cases.

The hunter's analysis is accurate. Callers of `get_proposal_content` cannot distinguish a recoverable `InvalidProposal` from a genuine internal batcher failure. No test is provided, but the code path is directly readable and the asymmetry with `finish_proposal` is confirmed.

**Fix suggestion**: Apply `proposal_status_from(err)?` inside `get_proposal_content`'s map_err, paralleling `finish_proposal`.

---

## Hunter 3 — apollo_gateway

### H3-B1: `P2pPropagatorClientError` returns internal error in deprecated gateway path

**Verdict**: `confirmed`

**What I checked**:
`errors.rs:237–240` in `mempool_client_result_to_gw_spec_result` returns `Ok(())` for `P2pPropagatorClientError` with a comment "Not an error from the gateway's perspective." Lines 286–293 in `mempool_client_err_to_deprecated_gw_err` handle the same variant by returning `StarknetError::internal_with_signature_logging(...)` — a full internal server error, despite the identical comment. `add_tx_inner` uses the deprecated path via `mempool_client_result_to_deprecated_gw_result`. The asymmetry is exact.

The proposed test directly constructs the error and calls both functions — no private-state access needed. It confirms the behavioral difference. The user-impact claim is also accurate: the user gets an error even though the tx was accepted by the mempool, and retrying causes a duplicate-transaction error.

**Fix suggestion**: In `mempool_client_err_to_deprecated_gw_err`, treat `P2pPropagatorClientError` the same as the spec path: log a warning and return `Ok(())` (or a non-error `StarknetError` that the caller converts to a success response).

---

### H3-B2: Nonce range check wraps on Felt overflow near field prime

**Verdict**: `confirmed`

**What I checked**:
`stateful_transaction_validator.rs:288–290`:
```rust
let max_allowed_nonce = Nonce(account_nonce.0 + Felt::from(self.config.max_allowed_nonce_gap));
if !(account_nonce <= incoming_tx_nonce && incoming_tx_nonce <= max_allowed_nonce)
```
`Felt` arithmetic is modular over the STARK prime. If `account_nonce.0` is near the prime, adding `max_allowed_nonce_gap` wraps to a small value. The subsequent comparison `account_nonce <= max_allowed_nonce` becomes `(P-1) <= small_number` which is `false`, causing even `incoming_tx_nonce == account_nonce` to fail.

The practical impact is minimal — a nonce near the STARK prime (~2^251) is physically unreachable in practice (it would require more transactions than any chain will ever process). However, the code is technically incorrect and the hunter's analysis is accurate.

The proposed test is mechanically legitimate (using `MockGatewayFixedBlockStateReader` and constructing an account with a maximal nonce), though it would require resolving exact type signatures. The field-overflow behavior is confirmed by reading the arithmetic directly.

**Fix suggestion**: Cap `max_allowed_nonce` to the field's maximum representable value: use saturating addition or check for overflow before constructing `max_allowed_nonce`.

---

### H3-B3: `max_nonce_for_validation_skip` config field has no runtime effect

**Verdict**: `confirmed`

**What I checked**:
`grep -rn "max_nonce_for_validation_skip" crates/apollo_gateway/` returns zero runtime reads — only config definition and serialization sites. The `skip_stateful_validations` function at `stateful_transaction_validator.rs:437` hardcodes `Nonce(Felt::ONE)` instead of reading `self.config.max_nonce_for_validation_skip`. The field is defined in `apollo_gateway_config/src/config.rs:256`, serialized in the config schema (`config_schema.json:3092`), and set to `0x1` in deployment configs — but changing it has no effect on behavior. This is a confirmed silent configuration lie.

**Fix suggestion**: Replace the hardcoded `Nonce(Felt::ONE)` in `skip_stateful_validations` with `self.config.max_nonce_for_validation_skip` (passed as a parameter or held in a captured reference), matching the pattern used in `native_blockifier/src/py_validator.rs:114`.

---

### H3-B4: `mempool_client_result_to_gw_spec_result` is dead code

**Verdict**: `confirmed`

**What I checked**:
`grep -rn "mempool_client_result_to_gw_spec_result" crates/` returns exactly one result — the function definition at `errors.rs:212`. No callers exist anywhere in the codebase. The function is `pub` and implements the correct (non-fatal) P2P error treatment, but it is never invoked.

This is genuine dead code confirming that the spec-gateway error path was partially implemented but never wired up. Combined with H3-B1, the consequence is that only the incorrect deprecated path is ever executed.

**Fix suggestion**: Either wire `mempool_client_result_to_gw_spec_result` into the actual call sites (replacing or supplementing the deprecated path), or remove it and consolidate the P2P non-fatal logic into the deprecated path directly.

---

### H3-B5: Calldata length check conflates proof_facts, error label is misleading

**Verdict**: `confirmed`

**What I checked**:
`stateless_transaction_validator.rs:165–167`:
```rust
RpcTransaction::Invoke(RpcInvokeTransaction::V3(tx)) => {
    tx.calldata.0.len() + tx.proof_facts.0.len()
}
```
This combined total is compared against `max_calldata_length` and reported in `CalldataTooLong { calldata_length: total_length, ... }`. The field name `calldata_length` is misleading since the value includes `proof_facts`. The hunter's test (4 calldata + 3 proof_facts = 7, max_calldata_length = 5) correctly demonstrates rejection of a tx whose calldata alone (4) is within the declared limit.

The test uses public types and the `StatelessTransactionValidator` public interface — no artificial state. The behavior is real and confirmable by reading the code.

**Fix suggestion**: Either rename `calldata_length` in the error to `total_calldata_and_proof_facts_length`, or split the check into two separate limits, or add documentation making the combined constraint explicit.

---

## Hunter 4 — apollo_consensus

### H4-B1: Integer overflow in `should_cache_msg` round-limit check

**Verdict**: `confirmed`

**What I checked**:
`manager.rs:384`:
```rust
height_diff == 0 && msg_round <= current_round + limits.future_round_limit
```
Both `current_round` and `future_round_limit` are `u32`. In debug builds, Rust panics on integer overflow. In release builds, it wraps silently. The overflow path requires `current_round` near `u32::MAX`, which in practice would require billions of rounds — extremely unlikely in normal operation, but possible as a DoS vector if a crafted message with a large `msg_round` is processed while `current_round` is large.

The hunter's proposed test (`current_round = u32::MAX, future_round_limit = 1`) is a valid arithmetic demonstration, not a system test. The test would pass (verifying the panic in debug mode). The overflow in release mode would cause `msg_round <= 0` instead of `msg_round <= u32::MAX + 1 - wraps_to_0`, which silently accepts votes that should be dropped.

**Fix suggestion**: Replace `current_round + limits.future_round_limit` with `current_round.saturating_add(limits.future_round_limit)`.

---

### H4-B2: Late-duplicate stream message destroys active stream state

**Verdict**: `confirmed`

**What I checked**:
`stream_handler.rs:518–523`: the `Ordering::Less` arm returns `None`. The caller at lines 397–423 patterns on the return:
```rust
let Some(data) = self.handle_message_inner(...) else {
    return Ok(());
};
```
When `None` is returned, `data` (including its `message_buffer` and the pending `Receiver`) is dropped. The stream is *not* reinserted into the LRU cache. Future messages for the same `(peer_id, stream_id)` pair find no entry in the cache, triggering creation of a fresh `StreamData` with `next_message_id = 0` and sending a *new* `Receiver` to the application.

The hunter's trace is correct: a retransmitted stale message (duplicate message_id 0 after message_id 0 was already processed) drops the `StreamData` that contained buffered messages 2 and 3. The application ends up with two receivers for the same logical stream; the old receiver has delivered message 0 and will deliver nothing more; the new receiver waits for a message_id 0 that will never arrive.

No mechanical test is provided, but the code path is unambiguous. The fix is trivial: `Ordering::Less` should return `Some(data)` to preserve stream state.

**Fix suggestion**: Change `return None;` to `return Some(data);` in the `Ordering::Less` arm.

---

### H4-B3: `inbound_send` returns `false` for Fin, conflating success with error

**Verdict**: `rejected`

**What I checked**:
`stream_handler.rs:274–278` returns `false` for `Fin`. The caller at lines 478–487 uses `!message_sent` as a condition to close the stream and return `None`. When a Fin arrives in order, `inbound_send` returns `false`, `!message_sent` is `true`, the stream is closed, and the stream-finished metric/log is emitted — which is exactly the correct behavior.

The hunter calls this a "design fragility" because the boolean conflates three states. However:
1. The current behavior is correct: the code works as intended.
2. The hunter explicitly states "Today the code happens to work."
3. The scenario where this causes a bug ("if `inbound_send` is ever called on a buffered Fin") is explicitly noted as "currently impossible."

A design concern about API clarity does not constitute a bug. There is no demonstrated failure path, and the hunter provides no test. This is a refactoring suggestion, not a bug.

---

### H4-B4: `handle_vote_broadcasted` panics for observer SHC instances

**Verdict**: `suspected`

**What I checked**:
`single_height_consensus.rs:173`: `last_vote.expect("No last vote to send")` panics if `last_self_prevote()` or `last_self_precommit()` returns `None`. `state_machine.rs:259–261` confirms observers return immediately from `make_self_vote` without setting `last_self_prevote` or `last_self_precommit`. So the panic path is real.

The question is whether `VoteBroadcasted` can reach an observer's SHC. In `manager.rs:742–744`, `shc_events.next()` feeds events to `shc.handle_event(shc_event)`. The `shc_events` queue is populated by `execute_requests` which schedules `VoteBroadcasted` when `SMRequest::BroadcastVote` is generated — but `SMRequest::BroadcastVote` is only generated from `make_self_vote`, which returns early for observers without emitting it. So in normal operation, no `BroadcastVote` is ever scheduled for an observer, and `VoteBroadcasted` never reaches the observer's SHC.

The hunter's test manufactures the condition by directly calling `shc.handle_event(StateMachineEvent::VoteBroadcasted(...))` on an observer SHC — a call that would not happen through the normal manager dispatch path. This is a case of the test reaching into an artificially constructed state.

The panic is real code, but the trigger requires a routing failure (a bug elsewhere in the manager). The bug is therefore "suspected": the defensive coding is poor (the function should handle the case gracefully), but it cannot be demonstrated without either a manager-level routing bug or an artificial test injection.

**What would make it confirmable**: Identify a code path in the manager that routes `VoteBroadcasted` to an observer SHC without bypassing the SHC event dispatcher — e.g., a message from a stale future arriving after the node transitions to observer status mid-height, if the shc_events FuturesUnordered is not cleared at that point.
