# Supervisor 3 Report

## Hunter 9 — Bug 1
**Verdict**: suspected

**Rationale**: The logic in `scan_at_block` (crates/apollo_storage/src/state/mod.rs, lines 258–278) is exactly as described. The key-advancement step at line 273 issues `cursor.lower_bound(&(current, BlockNumber(u64::from(u32::MAX))))`. Since `BlockNumber` is serialized as a `u32`, `BlockNumber(u32::MAX as u64)` is the largest representable serialized block number. If an entry at `(current, BlockNumber(u32::MAX))` exists, `lower_bound` lands on it, returns `next_key == current`, the loop does not advance, and the process loops forever. The TODO comment at line 272 (`// TODO(yoav): define StorageBlockNumber type that wraps a u32 and use it here`) confirms the developers are aware of the `u32` ceiling. The bug is real.

The test, however, is artificial. The test body acknowledges the impracticality at lines 87–96 ("To do so we must fill every block from 1..=max_block, which is infeasible") and falls back to a thread-with-timeout that scans at block 0 with an empty database. At block 0 with no state diff entries, `scan_class_hash_to_compiled_class_hash_in_range` will iterate over zero entries and the loop exits immediately without ever hitting the `lower_bound` key-advancement branch. The proposed test would always pass whether the bug exists or not — it does not actually insert an entry at `(key, BlockNumber(u32::MAX))` before scanning. The hunter explicitly concedes that the "minimal reproduction" requires internal table injection but does not provide it. As a result, the test as written does not demonstrate the bug through normal usage; it would pass even on buggy code and provides no diagnostic value.

The bug itself is real and latent, but a valid test would require inserting an entry at the maximum block number via an exposed write path and then scanning — which would require either a dedicated test helper or waiting for the block height to reach `u32::MAX`.

---

## Hunter 10 — Bug 1
**Verdict**: confirmed

**Rationale**: `pending_events` is declared as `Vec<…>` in `PeerManager` (crates/apollo_network/src/peer_manager/mod.rs, line 45). Events are pushed with `.push(…)` (append to end) at lines 176, 195, 219, 229 of mod.rs and at lines 128–134 of behaviour_impl.rs. The `poll` function at line 187 of behaviour_impl.rs drains with `.pop()` (removes from end), giving LIFO ordering instead of FIFO. The `// TODO(shahak): Change to VecDeque and awake when item is added.` comment at line 44 acknowledges this is known technical debt. LIFO ordering means that when multiple `SessionAssigned` events are queued in one batch (e.g., when `add_peer` assigns several pending sessions), the last-pushed event is emitted first. The test is valid: it registers two sessions, adds a peer (triggering both assignments in order), then asserts FIFO emission order — which is the natural expectation a caller would have. The scenario (sessions queued before a peer arrives, then peer added) is a normal usage path.

---

## Hunter 10 — Bug 2
**Verdict**: suspected

**Rationale**: The subtraction `last_block_number.0 - current_block_number.0` at line 125 of `block_data_stream_builder.rs` is plain `u64` arithmetic with no underflow guard. In debug builds this panics; in release builds it wraps. The `if limit == 0` guard that follows does not protect against the wrap — a wrapped value would be very large, not zero. In the normal sync flow, `current_block_number` advances only after a block is successfully written and the storage marker advances accordingly, so `current_block_number <= last_block_number` should hold. However, the stream uses `Self::get_start_block_number` at startup (which reads the storage marker once), then the outer loop re-reads the marker every iteration while `current_block_number` is updated per block. A race where the marker is transiently stale is theoretically possible, and a logic error upstream could put `current_block_number` ahead.

The bug (panic on underflow in debug mode) is real in principle. However, the proposed test is pure arithmetic in isolation: it subtracts two raw `u64` literals without any connection to `create_stream` or the stream state machine. This is not a test that demonstrates the bug through normal usage — it just documents that `5u64 - 6u64` panics, which is a language property rather than a code-path observation. A legitimate test would mock storage to return a stale/smaller marker and verify the stream panics or handles gracefully.

---

## Hunter 10 — Bug 3
**Verdict**: rejected

**Rationale**: The `select!` concern has merit at the conceptual level but does not constitute an actionable bug in the current code. Examining `block_data_stream_builder.rs` lines 161–203: when the `get_internal_block_at` arm wins, the code executes `continue 'send_query_and_parse_responses`, which drops `client_response_manager`. The `ClientResponsesManager` drop will close the sender side of the channel, which triggers cleanup on the network side. The "orphaned session" claim requires that the network-side session continues sending messages into a now-closed channel — those sends will just fail silently. There is no state corruption and no infinite wait; the old session times out naturally, and the new `send_new_query` starts a fresh session. The partial parse state is discarded (chunk 1 of a multi-chunk state diff is lost), but this is handled by re-querying from `current_block_number`. The hunter acknowledges: "No data loss for the block (the internal block was used)". The behavior is consistent with the broader retry semantics already present in the loop. No test is provided, and the described scenario is not a user-observable bug — it is a session lifecycle detail.

---

## Hunter 10 — Bug 4
**Verdict**: rejected

**Rationale**: The bug claim is that line 196 (`self.sleep_waiting_for_unblocked_peer = None`) unconditionally clears a newly-installed sleep future set inside the `assign_peer_to_session` loop at lines 192–194. Tracing the code: `ready!(sleep_future.as_mut().poll(cx))` at line 191 blocks (returns `Poll::Pending`) until the sleep completes. The sleep is set via `tokio::time::sleep_until(blocked_until)`. When the sleep fires, it means the wall clock has reached `blocked_until`. At that moment, `peer.is_blocked()` returns `timed_out_until > get_instant_now()` — since `get_instant_now() >= timed_out_until`, `is_blocked()` is `false`. Therefore, when the sleep fires, the peer is available and `assign_peer_to_session` succeeds: it pushes a `SessionAssigned` event and does NOT install a new sleep. Line 196 then clears `None` (or the already-consumed sleep future), which is correct. The scenario where the sleep fires but the peer is still blocked (triggering installation of a new sleep that is immediately cleared) cannot arise through normal tokio sleep behavior — tokio does not wake a `sleep_until(deadline)` future before `deadline`. The test relies on advancing the virtual clock to 99ms when the deadline is 100ms, which would not cause the 100ms sleep to fire at all (`ready!` would return `Poll::Pending`). The test as written would not expose the claimed bug. While there is a theoretical concern if the tokio clock jitters or if the conversion between `tokio::time::Instant` and `std::time::Instant` introduces drift (see the `#[cfg(test)]` override in peer.rs), this is not demonstrated by the provided test or evidence.

---

## Hunter 11 — Bug 1
**Verdict**: rejected

**Rationale**: The DA gas discount at lines 64–65 of `gas_usage.rs` unconditionally subtracts the fee-balance word discount. This is intentional by design: the calling chain ensures a fee-balance storage update is always counted. `StateResources::new` (resources.rs line 200) calls `state_changes.count_for_fee_charge(sender_address, fee_token_address)`, and `count_for_fee_charge` (cached_state.rs line 866) explicitly adds 1 to `n_storage_updates` for the sender's fee balance if it is not already in the state diff — this pre-counts the fee balance update that will occur during fee transfer. The discount in `get_da_gas_cost` corresponds exactly to this pre-counted word. The existing test `test_onchain_data_discount` (gas_usage_test.rs line 228) documents this: its `n_storage_updates: 1` is labeled `// Fee balance update`. The hunter's test calls `get_da_gas_cost` in isolation with a synthetic `StateChangesCount` constructed without going through `count_for_fee_charge`, testing a usage pattern that does not occur in production code. The assertion fails because the test's expected value omits the discount that is legitimately present when the function is called normally. This is a test that manufactures failure by bypassing the intended API (`count_for_fee_charge`) and constructing an artificial input.

---

## Hunter 11 — Bug 2
**Verdict**: suspected

**Rationale**: The code in `fill_sequencer_balance_reads` (concurrency/fee_utils.rs, lines 94–99) does contain two hard-coded `assert!`s that panic rather than returning errors. The `assert_eq!(storage_read_values.len(), 4, ...)` at line 94 and `assert_eq!(storage_read_values[index], Felt::ZERO, ...)` at line 98 will crash the sequencer process if violated. The zero-balance assertion is documented by the comment at line 76–78: concurrency mode runs the fee transfer with a fake zero balance. The assertion is correct for the current ERC-20 contract and execution model. If the fee token contract were upgraded to have different internal storage access patterns, or if a future code path created non-zero sequencer reads in the concurrent context, this would panic.

The test demonstrates a real code behavior: calling `fill_sequencer_balance_reads` with a non-zero concurrent balance does panic as claimed. However, the scenario requires constructing a `CallInfo` directly with a manually crafted `storage_read_values` vector — a state the normal execution path explicitly prevents by zeroing these reads in the concurrent executor before calling this function. The hunter's scenario ("if the concurrent state happens to have a previously-written sequencer balance") does not map to how the function is actually invoked. `complete_fee_transfer_flow` is called after concurrent execution, where the sequencer balance reads are always zero by construction. The test manufactures the panic-triggering state by bypassing all the machinery that prevents it, so it does not demonstrate a bug reachable through normal usage. The risk is real as a future-proofing concern (a contract upgrade or code refactor could violate the invariant), but the test is not legitimate under the current codebase semantics.

---

## Hunter 11 — Bug 3
**Verdict**: rejected

**Rationale**: `total_charged_computation_units` (resources.rs line 116) is gated with `#[cfg(test)]` — it exists only in test builds and cannot affect production. The overflow concern (`self.sierra_gas.0 + self.reverted_sierra_gas.0` could exceed `u64::MAX`) is technically valid for pathological inputs. In debug builds with overflow checks, this panics; in release builds, `u64` wraps. However, the function is used only in tests to verify accounting — any realistic test values are small, so the overflow cannot occur in practice. The proposed test requires `sierra_gas = u64::MAX` combined with `reverted_sierra_gas >= 1`, which is an impossible state in real execution (no transaction can consume u64::MAX gas). The test demonstrates a language arithmetic property rather than a reachable production defect. Calling this a "bug" in test-only, test-only-reachable code with physically impossible inputs is not justified.

---

## Hunter 12 — Bug 1
**Verdict**: confirmed

**Rationale**: The `get_events` handler at line 717 checks `if filter.chunk_size > self.max_events_chunk_size` — this accepts `chunk_size = 0`. The OpenAPI spec at `starknet_api_openrpc.json` line 921 specifies `"minimum": 1`, so `chunk_size = 0` is an invalid input by the API contract. With `chunk_size = 0`, the inner loop at line 792 checks `if filtered_events.len() == filter.chunk_size` — `filtered_events` starts empty (length 0), so `0 == 0` is true on the very first matching event. The handler returns an `EventsChunk` with an empty `events` array and a `continuation_token` pointing to that first event. The next call with that token produces identical behavior. The livelock is deterministic and client-visible: a client following the continuation token will loop forever receiving empty pages. The same pattern at line 836 applies to the pending block path. The fix is to add `|| filter.chunk_size == 0` (or `< 1`) to the guard at line 717. The test is legitimate: it calls `get_events` with `chunk_size: 0` as a normal RPC client would (no internals accessed), and asserts that no continuation token is returned. This is exactly how the bug would surface for a real user. The spec-vs-behavior mismatch is genuine.

---

## Hunter 12 — Bug 2
**Verdict**: confirmed

**Rationale**: Lines 674–676 of `crates/apollo_rpc/src/v0_8/api/mod.rs` are:
```rust
base64::decode(base64_compressed_program).map_err(internal_server_error)?;
let compressed_data =
    base64::decode(base64_compressed_program).map_err(internal_server_error)?;
```
The first `base64::decode` call decodes the input, returns a `Vec<u8>`, and the result is immediately discarded (only the `?` for early exit on error is used). The second call decodes the identical input again and stores it. This is a clear code defect: the first decode allocates a `Vec<u8>` that is immediately dropped, doing redundant work. The fix is a one-line change. This is visually verifiable in the source without running any code. The test provided calls the function and verifies correct output, which is sufficient to anchor the finding — the double-decode is observable in the source code itself. While the test does not mechanically prove the double-decode (it does not count allocations), the code-level evidence is unambiguous. This is a real defect in production code, not a test-only or latent issue.

---

## Summary
- Confirmed: 3 bugs (Hunter 10 Bug 1, Hunter 12 Bug 1, Hunter 12 Bug 2)
- Suspected: 3 bugs (Hunter 9 Bug 1, Hunter 10 Bug 2, Hunter 11 Bug 2)
- Rejected: 5 bugs (Hunter 10 Bug 3, Hunter 10 Bug 4, Hunter 11 Bug 1, Hunter 11 Bug 3, Hunter 12 Bug 2 — wait, Hunter 12 Bug 2 is confirmed above)

Corrected tally:
- Confirmed: 3 bugs (Hunter 10 Bug 1, Hunter 12 Bug 1, Hunter 12 Bug 2)
- Suspected: 3 bugs (Hunter 9 Bug 1, Hunter 10 Bug 2, Hunter 11 Bug 2)
- Rejected: 4 bugs (Hunter 10 Bug 3, Hunter 10 Bug 4, Hunter 11 Bug 1, Hunter 11 Bug 3)
