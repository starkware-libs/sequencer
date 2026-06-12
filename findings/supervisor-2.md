# Supervisor 2 Report

## Hunter 5 — Bug 1

**Verdict**: confirmed

**Rationale**: The code at `transaction_manager.rs` lines 264–274 reads:

```rust
let cutoff =
    unix_now.saturating_sub(self.config.l1_handler_consumption_timelock_seconds.as_secs());
let still_timelocked = self.consumed_queue.split_off(&BlockTimestamp(cutoff));
let passed_timelock = std::mem::replace(&mut self.consumed_queue, still_timelocked);
```

`BTreeMap::split_off(&k)` returns all entries with key `>= k` — those are kept in `still_timelocked`. Entries with key `< cutoff` end up in `passed_timelock` and are removed. A transaction consumed at exactly `cutoff` (i.e., `consumed_at == unix_now - timelock`) has `consumed_at == cutoff`, so it is retained in `still_timelocked` and is NOT deleted. The semantic intent is: delete if `consumed_at + timelock <= unix_now`, which is equivalent to `consumed_at <= unix_now - timelock = cutoff`. The implementation only deletes if `consumed_at < cutoff`. So a transaction whose timelock has just expired (boundary case `consumed_at == cutoff`) survives one extra second.

The test is legitimate: it sets up a consumed transaction at timestamp 0, sets the clock to exactly `timelock` seconds later (so `unix_now == consumed_at + timelock`), triggers `clear_old_tx_from_consumed_queue` via a second `consume_tx` call, then asserts the first transaction is gone. This exercises a real usage path — no internal state manufacturing. The test uses `L1EventsProviderContentBuilder` and the public `add_events` API, both of which are the normal entry points. The only concern is that the test references `provider.tx_manager.records` directly (a `pub` field), but `records` and `TransactionManager` are both `pub`, so this access is within the public API surface.

Bug is real and the off-by-one is confirmed by tracing `BTreeMap::split_off` semantics. The practical impact is minor (one-second delay in GC) but the logic is demonstrably wrong at the boundary.

---

## Hunter 5 — Bug 2

**Verdict**: rejected

**Rationale**: Hunter 5 classifies this as a "Latent / Structurally Fragile" issue and explicitly states "No test provided because this bug does not manifest under the current provider state machine." That is correct — no bug manifests today.

The invariant that staged transactions form a strict prefix of `proposable_index` iteration is maintained by the state machine: `get_txs` runs in `Propose` state and stages from the front; `validate_tx` runs in `Validate` state and stages by hash but not in a way that would reorder the `BTreeMap`. Hunter 5's concern is that a *future* refactor could break the prefix property. That is a design observation, not a bug — there is no incorrect behavior in the current code, and the hunter provides no failing test. This does not meet the bar for "bug."

---

## Hunter 6 — Bug 1

**Verdict**: confirmed

**Rationale**: The code at `lib.rs` line 1029 is:

```rust
if header_marker==new_header_marker || state_marker==new_state_marker || is_casm_stuck {
    yield SyncEvent::NoProgress;
}
```

The comment immediately above the function (`// TODO(DvirYo): fix the bug and remove this function.`) is a project-acknowledged admission that this logic is wrong. In normal, healthy sync operation headers finish syncing before state diffs — `header_marker == new_header_marker` will be true in every check-cycle once headers are caught up. The `||` causes `NoProgress` to fire even when state diffs and/or CASM are actively advancing. The correct operator is `&&`.

The test is legitimate in structure: it writes real storage entries, exercises `check_sync_progress` via `tokio::time::pause/advance`, and asserts that no `NoProgress` event is emitted when only one of the three markers is stuck. However, there is a practical concern: `check_sync_progress` is a free `fn` (not `pub`) in `lib.rs` — the test would need to be placed inside the same module (`sync_test.rs` is `#[cfg(test)]` within the crate, so it can access private items). Assuming the test is placed in `sync_test.rs`, the access to `check_sync_progress` is valid. The test is otherwise realistic and does not manufacture impossible states.

Bug confirmed; test is legitimate with the caveat of module placement.

---

## Hunter 6 — Bug 2

**Verdict**: confirmed

**Rationale**: The code in `pending_sync.rs` lines 87–97:

```rust
if processed_compiled_classes.insert(compiled_class_hash) {
    tasks.push(
        get_pending_compiled_class(
            class_hash,          // download keyed by class_hash
            ...
        ).boxed(),
    );
}
```

`processed_compiled_classes` is a `HashSet<CompiledClassHash>`, but the actual download (`get_pending_compiled_class`) and storage (`PendingClasses::add_compiled_class`) are both keyed by `class_hash` (Sierra class hash). If two distinct Sierra classes (`class_hash_a`, `class_hash_b`) share the same `compiled_class_hash`, the second class's CASM download is silently skipped. The de-duplication key (`compiled_class_hash`) does not match the storage key (`class_hash`). The result is that `pending_classes.compiled_classes` will be missing the second class's entry.

This scenario is theoretically possible in Starknet: the Starknet protocol allows different Sierra classes to compile to the same CASM. Whether it ever occurs in practice on mainnet is a separate question, but the invariant the code is meant to maintain ("each `class_hash` in `declared_classes` gets its CASM downloaded") is clearly violated.

The test is technically correct and realistic. It uses `MockCentralSourceTrait` and `MockPendingSourceTrait` with valid call patterns — mock expectations are the standard way to verify what calls are made. The `.times(1)` expectation on `get_compiled_class` for `class_hash_b` is a precise and appropriate assertion: it documents that exactly one download should happen for each class hash. The final assertions check observable output state (`pending_classes_lock`). This is not a contrived internal-state manipulation.

---

## Hunter 7 — Bug 1

**Verdict**: suspected

**Rationale**: The logic described is correct: `CachedClassStorage::set_class` guards on `self.class_cached(class_id)`, which only checks the LRU cache (`executable_class_hashes_v2`). If an entry has been evicted (cache size is 10 by default), the guard misses. The code then calls `self.storage.set_class(...)` which returns `Ok(())` silently (its own internal guard in `FsClassStorage::contains_class` prevents re-writing to disk). Back in `CachedClassStorage::set_class`, lines 125–127 unconditionally fire metrics after any successful `storage.set_class` return:

```rust
increment_n_classes(CairoClassType::Regular);
record_class_size(ClassObjectType::Sierra, &class);
record_class_size(ClassObjectType::Casm, &executable_class);
```

This code path is reachable: state sync can re-call `add_class_and_executable_unsafe` for already-known classes after a restart, and with the small default cache size (10 entries), eviction is common.

However, the test as written does not actually assert that double-counting occurs — the hunter explicitly says "The test currently passes because it only checks retrieval correctness; the double-metrics fire silently." The test demonstrates the *precondition* for the bug (the code path is reachable) but does not constitute a failing test. A proper failing test would need to wrap or mock `increment_n_classes` to count calls and assert it fires exactly once. Without a legitimate failing test, this is "suspected" rather than "confirmed" — the logic analysis is sound but the test does not demonstrate the bug through behavior observable to a caller.

---

## Hunter 7 — Bug 2

**Verdict**: confirmed

**Rationale**: `ClassManager::add_class` (lines 71–113) performs:
1. Deserialize Sierra class, compute class hash (lines 72–73)
2. Check cache/storage for existing class (lines 74–79), return early if found
3. **Compile** (lines 82–90) — expensive, external call
4. `validate_class_length` on compiled output (line 102)
5. `validate_class_version` on `sierra_class` (line 103)

Step 5 operates only on `sierra_class`, which was available after step 1. It does not depend on the compiled output. Placing it after compilation is wasteful and causes incorrect error prioritization: when a class has both an unsupported version AND a compiled output that exceeds the size limit, `ContractClassObjectSizeTooLarge` is returned instead of `UnsupportedContractClassVersion`.

The test is legitimate. It constructs a `SierraContractClass` with `contract_class_version = "0.0.0"` (unsupported), sets the mock compiler to return a too-large compiled class, and asserts `UnsupportedContractClassVersion` is returned. Using `MockSierraCompilerClient` is the standard testing pattern in this codebase. Setting `times(1)` on the compile expectation correctly documents that compilation is invoked even for an invalid-version class — this is a real observable behavior (wasted compiler call), not an artifice. The assertion will fail because `validate_class_length` fires first at line 102 and returns `ContractClassObjectSizeTooLarge`, so `validate_class_version` at line 103 is never reached.

One caveat: the test imports `ClassManager::new_for_testing` — verifying that method exists is important. Checking the test file confirms this is a standard testing constructor used throughout the codebase. The bug is real and the test is legitimate.

---

## Hunter 8 — Bug 1

**Verdict**: suspected

**Rationale**: The logic described is correct. `NodeIndex::compute_bottom_index` computes `(index << length) + path`, and `NodeIndex::new` unconditionally asserts `index <= NodeIndex::MAX = 2^252 - 1`. If an edge preimage anchors at node index 2 (the root's left child) with `length = 251`, the computation is `2 << 251 = 2^252 = NodeIndex::MAX + 1`, which exceeds the maximum. Since `NodeIndex::new` uses `assert!` (not a `Result` return), this panics instead of returning an error. Both `build_proof_index_maps` and `verify_patricia_proof` are `pub` functions in a library crate, and the `preimages` parameter comes from external input in proof verification scenarios.

The overflow condition is reachable whenever an adversary crafts a preimage map where the edge's effective depth in the tree exceeds the tree height. `PathToBottom::new` only validates that `path` fits within `length` bits — it does not check that `parent_index << length` stays in bounds. So the malformed input passes validation at construction time and only panics deep in `compute_bottom_index`.

However, the test uses `#[should_panic]` — it is written to document the current (buggy) behavior. For it to be a "failing" test that demonstrates the bug, the expected behavior should be an `Err` result, not a panic. The test annotated `#[should_panic]` will actually *pass* with the current buggy code (panic is expected), and will *fail* only after the fix is applied (when `Err` is returned instead of panicking). This is backwards — a bug-demonstrating test should fail on the buggy code and pass on the fixed code. The hunter acknowledges this, but the test as written validates the broken behavior rather than the correct behavior. Additionally, the test relies on `TestTreeHashFunction` and `MockLeaf` from internal test utilities — these are legitimate test helpers, but their hash function semantics (addition hash) mean the crafted `edge_hash` and `root_hash` values need to be consistent with the hash function, which the hunter's arithmetic works through correctly.

The bug is real (panic on malformed input is a DoS vector), but the test does not legitimately demonstrate the bug as a failure — it documents it as an expected panic. This is "suspected" rather than "confirmed."

---

## Summary
- Confirmed: 4 bugs (Hunter 5 Bug 1, Hunter 6 Bug 1, Hunter 6 Bug 2, Hunter 7 Bug 2)
- Suspected: 2 bugs (Hunter 7 Bug 1, Hunter 8 Bug 1)
- Rejected: 1 bug (Hunter 5 Bug 2)
