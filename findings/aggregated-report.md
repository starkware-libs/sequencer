# Apollo Sequencer Bug Hunt — Aggregated Report

**Date**: 2026-06-12  
**Method**: 16 bug hunters × 4 supervisor validators  
**Total verdicts**: 14 confirmed · 10 suspected · 6 rejected

---

## Summary Table — Confirmed Bugs

| # | Component | File | Severity | Description |
|---|-----------|------|----------|-------------|
| C1 | `apollo_consensus` | `state_machine.rs` | **CRITICAL** | Liveness violation: round-skip uses OR instead of combined vote weight (Tendermint LOC 55) |
| C2 | `apollo_consensus_orchestrator` | `sequencer_consensus_context.rs` | **HIGH** | Inverted Less/Greater arms silently delete future-round proposals |
| C3 | `apollo_batcher` | `batcher.rs` | **HIGH** | Panic at protocol-version transition (missing PartialBlockHashComponents) |
| C4 | `apollo_central_sync` | `lib.rs` | **HIGH** | Sync progress watchdog uses `\|\|` instead of `&&` → spurious restarts |
| C5 | `apollo_central_sync` | `pending_sync.rs` | **HIGH** | CASM deduplication uses wrong key (CompiledClassHash instead of ClassHash) |
| C6 | `apollo_gateway` | `stateless_transaction_validator.rs` | **HIGH** | min_gas_price check incorrectly rejects valid L1-only V3 transactions |
| C7 | `apollo_mempool` | `mempool.rs` | **MEDIUM** | Stale user nonce used for gap detection, rejecting valid gap-closers as MempoolFull |
| C8 | `apollo_network` | `peer_manager/behaviour_impl.rs` | **MEDIUM** | LIFO event queue (`Vec::pop`) instead of FIFO for session assignment |
| C9 | `apollo_rpc` | `api_impl.rs` | **MEDIUM** | `get_events` with `chunk_size=0` loops forever (continuation token never advances) |
| C10 | `apollo_l1_events` | `transaction_manager.rs` | **MEDIUM** | Off-by-one: consumed tx with expiry at exactly cutoff survives cleanup |
| C11 | `apollo_class_manager` | `class_manager.rs` | **MEDIUM** | Wrong validation order: version check runs after compilation, returns wrong error |
| C12 | `apollo_consensus_orchestrator` | `utils.rs` | **MEDIUM** | `truncate_to_executed_txs(0)` returns `vec![vec![]]` instead of `vec![]` |
| C13 | `starknet_api` | `transaction/fields.rs` | **MEDIUM** | `Fee::checked_div_ceil` overflows when floor quotient == u64::MAX with remainder |
| C14 | `apollo_rpc` | `api/mod.rs` | **LOW** | `decompress_program` decodes base64 twice, discarding the first result |

---

## Confirmed Bugs — Details

### C1 · CRITICAL · `apollo_consensus` — Liveness Violation in Round-Skip Logic

**File**: `crates/apollo_consensus/src/state_machine.rs` ~line 720  
**Root cause**: `maybe_advance_to_round` checks prevote weight OR precommit weight independently against the round-skip threshold. Tendermint Algorithm 1 Line 55 specifies **any** message type combined. In a 4-validator equal-weight network (threshold = 2), a split of 1 prevote + 1 precommit for a future round (combined = 2 ≥ threshold) never advances — liveness failure.

**Fix**: Sum weight across both vote types before comparing to threshold.

```rust
// BEFORE (buggy)
if self.round_has_enough_votes(&self.prevotes, round, &self.round_skip_threshold)
    || self.round_has_enough_votes(&self.precommits, round, &self.round_skip_threshold)

// AFTER (correct)
let combined_weight = self.prevotes.get_weight_for_round(round)
    + self.precommits.get_weight_for_round(round);
if combined_weight >= self.round_skip_threshold
```

**Test**: `round_skip_threshold_must_combine_prevote_and_precommit_weight` in `hunter-4.md`

---

### C2 · HIGH · `apollo_consensus_orchestrator` — Inverted Match Arms Delete Future Proposals

**File**: `crates/apollo_consensus_orchestrator/src/sequencer_consensus_context.rs` lines 1092–1102  
**Root cause**: The `while let Some(entry) = self.queued_proposals.first_entry()` loop iterates from the smallest key. The `Less` and `Greater` match arms are **swapped**:
- `Less` (future round) → **removes** the entry (should break/keep)
- `Greater` (stale past round) → **returns early** (should remove and continue)

When a node queues a proposal for round 4 while at round 0, then advances to round 2, the round-4 entry is deleted. At round 4 the proposal is gone → `Err(Canceled)` → liveness failure.

**Fix**: Swap the action bodies in the `Less` and `Greater` arms.

**Test**: `queued_proposal_for_future_round_is_not_dropped_on_intermediate_advance` in `hunter-14.md`

---

### C3 · HIGH · `apollo_batcher` — Panic at Protocol-Version Transition

**File**: `crates/apollo_batcher/src/batcher.rs`  
**Root cause**: `get_parent_proposal_commitment` calls `.expect("Missing partial block hash components for previous height.")` on the result of fetching `PartialBlockHashComponents`. Old-protocol blocks synced via `add_sync_block` never store these. The first new-protocol block proposal panics the batcher.

**Fix**: Return `Ok(None)` when `components` is `None` instead of panicking.

**Test**: `get_parent_proposal_commitment_panics_on_old_protocol_parent` in `hunter-3.md`

---

### C4 · HIGH · `apollo_central_sync` — Spurious Sync Restarts (Acknowledged TODO)

**File**: `crates/apollo_central_sync/src/lib.rs` line ~1029  
**Root cause**: 
```rust
if header_marker == new_header_marker || state_marker == new_state_marker || is_casm_stuck {
```
Uses `||` (OR). Headers finishing sync before state diffs is **normal operation**, but this fires `SyncEvent::NoProgress` every 5-minute check interval, causing spurious restarts. The code itself has a comment: `// TODO(DvirYo): fix the bug and remove this function.`

**Fix**: Change `||` to `&&` — only restart when ALL three streams are stuck simultaneously.

**Test**: `check_sync_progress_does_not_restart_when_only_headers_are_caught_up` in `hunter-6.md`

---

### C5 · HIGH · `apollo_central_sync` — Wrong Dedup Key for Pending CASM

**File**: `crates/apollo_central_sync/src/pending_sync.rs` lines 63–97  
**Root cause**: 
```rust
let mut processed_compiled_classes: HashSet<CompiledClassHash> = HashSet::new();
if processed_compiled_classes.insert(compiled_class_hash) {
    tasks.push(get_pending_compiled_class(class_hash, ...).boxed());
```
Deduplication uses `CompiledClassHash` as key, but `PendingClasses` stores by `ClassHash`. Two Sierra classes sharing a `CompiledClassHash` cause the second class's CASM to never be fetched. `get_compiled_class(class_hash_b)` returns `None`.

**Fix**: Use `ClassHash` as the dedup key instead of `CompiledClassHash`.

**Test**: `pending_sync_skips_second_class_with_shared_compiled_hash` in `hunter-6.md`

---

### C6 · HIGH · `apollo_gateway` — L1-Only Transactions Rejected by min_gas_price

**File**: `crates/apollo_gateway/src/stateless_transaction_validator.rs` lines 71–76  
**Root cause**: 
```rust
if resource_bounds.l2_gas.max_price_per_unit.0 < self.config.min_gas_price { .. }
```
Fires even when `l2_gas.max_amount == 0`. A valid `AllResources` V3 transaction that pays entirely in L1 gas sets `l2_gas = {max_amount: 0, max_price_per_unit: 0}`. The production default `min_gas_price = 8_000_000_000` rejects it with `MaxGasPriceTooLow`. Tests use `min_gas_price: 0` so this never triggers in CI.

**Fix**: Guard with `l2_gas.max_amount.0 > 0 &&` before the price comparison.

**Test**: `test_min_gas_price_incorrectly_rejects_l1_only_transaction` in `hunter-1.md`

---

### C7 · MEDIUM · `apollo_mempool` — Stale Nonce Causes Gap-Closer to be Rejected

**File**: `crates/apollo_mempool/src/mempool.rs` fn `handle_capacity_overflow` ~line 1011  
**Root cause**: The gap-detection predicate compares `tx.nonce() == account_nonce` where `account_nonce` is the **raw user-submitted** nonce, not the mempool's internally resolved nonce. A user submitting a gap-closer with a stale account nonce (common after a recently committed block they haven't seen) is misclassified as gap-creating → `MempoolFull`.

**Fix**: Use the mempool's resolved `next_nonce` for gap detection, not the raw submitted nonce.

**Test**: `gap_closing_tx_with_stale_account_nonce_succeeds` in `hunter-2.md`

---

### C8 · MEDIUM · `apollo_network` — LIFO Event Queue in PeerManager

**File**: `crates/apollo_network/src/peer_manager/behaviour_impl.rs` line ~187  
**Root cause**: `pending_events` is a `Vec`. Events are pushed with `.push()` (appended) and consumed with `.pop()` (end) — LIFO ordering. When multiple sessions are reassigned at once, `SessionAssigned` events arrive in reverse order. The TODO comment at line 44 acknowledges this. SQMR's own `behaviour.rs` correctly uses `VecDeque`/`pop_front`.

**Fix**: Change `pending_events: Vec<_>` to `VecDeque<_>`, and consume with `pop_front()`.

**Test**: `peer_manager_pending_events_are_fifo` in `hunter-10.md`

---

### C9 · MEDIUM · `apollo_rpc` — `get_events` Livelock with chunk_size=0

**File**: `crates/apollo_rpc/src/v0_8/api/api_impl.rs` lines ~792 and ~836  
**Root cause**: The input guard only rejects `chunk_size > max_events_chunk_size`, not `chunk_size == 0`. With `chunk_size = 0`, the `filtered_events.len() == filter.chunk_size` check (`0 == 0`) fires immediately before any event is pushed, returning an empty page with a non-advancing continuation token. Every subsequent call with that token repeats identically — infinite loop.

**Fix**: Add `filter.chunk_size == 0 ||` to the guard, returning an error.

**Test**: `get_events_with_chunk_size_zero_returns_error` in `hunter-12.md`

---

### C10 · MEDIUM · `apollo_l1_events` — Off-by-One in Consumed Tx Cleanup

**File**: `crates/apollo_l1_events/src/transaction_manager.rs` fn `clear_old_tx_from_consumed_queue` ~line 264  
**Root cause**: `BTreeMap::split_off(&BlockTimestamp(cutoff))` keeps entries with key `>= cutoff`. A transaction consumed at exactly `cutoff = unix_now - timelock` should be expired (`consumed_at + timelock == unix_now`) but survives until the next tick.

**Fix**: Use `split_off(&BlockTimestamp(cutoff + 1))` or `split_off(&BlockTimestamp(cutoff.saturating_add(1)))`.

**Test**: `test_consumed_tx_deleted_at_exact_timelock_expiry` in `hunter-5.md`

---

### C11 · MEDIUM · `apollo_class_manager` — Wrong Validation Order

**File**: `crates/apollo_class_manager/src/class_manager.rs` lines ~102–103  
**Root cause**: `validate_class_version` is called **after** Sierra-to-CASM compilation. A class with an unsupported version wastes a full compilation call. When both `UnsupportedContractClassVersion` and `ContractClassObjectSizeTooLarge` conditions hold, the caller receives the wrong error (size check runs first).

**Fix**: Move `validate_class_version` before the compilation call.

**Test**: `add_class_version_error_takes_precedence_over_size_error` in `hunter-7.md`

---

### C12 · MEDIUM · `apollo_consensus_orchestrator` — Spurious Empty Batch on Zero-Tx Reproposal

**File**: `crates/apollo_consensus_orchestrator/src/utils.rs` lines 452–473  
**Root cause**: `truncate_to_executed_txs(0)` returns `vec![vec![]]` instead of `vec![]`. The loop always enters the first batch and pushes `batch.into_iter().take(0).collect()` before breaking. On reproposal of an empty block, a spurious `ProposalPart::Transactions(vec![])` is sent that was absent from the original proposal stream.

**Fix**: Add an early return `if final_n_executed_txs == 0 { return vec![]; }`.

**Test**: `truncate_to_executed_txs_zero_returns_empty_outer_vec` in `hunter-14.md`

---

### C13 · MEDIUM · `starknet_api` — `Fee::checked_div_ceil` Integer Overflow

**File**: `crates/starknet_api/src/transaction/fields.rs` line ~58  
**Root cause**: When `floor(fee/price) == u64::MAX` with a non-zero remainder (ceiling would be `u64::MAX + 1`), the code executes `(value.0 + 1)` where `value.0: u64 = u64::MAX`. Debug: panic. Release: wraps to 0, returning `Some(GasAmount(0))` — silently wrong.

**Fix**: 
```rust
value.0.checked_add(1).map(|ceiled| GasAmount(ceiled))
```

**Test**: `checked_div_ceil_wraps_at_u64_max` in `hunter-13.md`

---

### C14 · LOW · `apollo_rpc` — Double Base64 Decode in `decompress_program`

**File**: `crates/apollo_rpc/src/v0_8/api/mod.rs` lines 674–676  
**Root cause**: The first `base64::decode(base64_compressed_program)` result is immediately discarded; only its error-propagation side-effect is used. An identical decode runs on the next line. Doubles CPU and heap allocation for every deprecated Cairo 0 declare transaction.

**Fix**: `let compressed_data = base64::decode(base64_compressed_program).map_err(internal_server_error)?;`

---

## Suspected Bugs (need further investigation)

| # | Component | Description | Why Suspected |
|---|-----------|-------------|---------------|
| S1 | `apollo_batcher` | TOCTOU race in `abort_proposal` | Real race, but test is non-deterministic |
| S2 | `apollo_batcher` | `send_txs_for_proposal` panic instead of error | Real path, no standalone test |
| S3 | `apollo_class_manager` | Metrics double-count on cache eviction | Logic sound, test doesn't verify counters |
| S4 | `starknet_patricia` | `build_proof_index_maps` panic on malformed proof (DoS) | Real panic, but test uses `#[should_panic]` (wrong direction) |
| S5 | `apollo_storage` | `scan_at_block` infinite loop at `BlockNumber(u32::MAX)` | Test doesn't trigger the loop |
| S6 | `apollo_p2p_sync` | u64 underflow in block limit calculation | Test is arithmetic isolation, not code path |
| S7 | `apollo_signature_manager` | Domain separation gap in peer identity signing | Test doesn't call `verify_identity` |
| S8 | `apollo_transaction_converter` | P2P-propagated txs miss proof manager | Feature acknowledged incomplete |
| S9 | `apollo_infra` | Dropped `JoinHandle` hides component panics | Test uses non-standard `catch_unwind` |
| S10 | `apollo_infra` | Off-by-one in retry attempt logging | Test only proves inline arithmetic |

---

## Rejected Bugs

| # | Component | Why Rejected |
|---|-----------|--------------|
| R1 | `apollo_l1_events` | `skip_while` vs `filter` — design observation, not current bug |
| R2 | `apollo_p2p_sync` | `select!` non-cancel-safety — no actual data loss occurs |
| R3 | `apollo_network` | Sleep sentinel clearing — logic is correct |
| R4 | `blockifier` | DA fee-balance discount — intentional, always paired with +1 in `count_for_fee_charge` |
| R5 | `blockifier` | `fill_sequencer_balance_reads` panic — unreachable through normal execution |
| R6 | `blockifier` | `total_charged_computation_units` overflow — test-only code, impossible inputs |
