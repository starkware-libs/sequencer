# Supervisor #3 Validation Report — Hunters 9–12

## Summary Table

| Bug ID | Title | Verdict | Severity |
|--------|-------|---------|----------|
| H9-B1 | Metric recorded even when broadcast fails or batch is dropped | confirmed | Medium |
| H9-B2 | Transactions permanently lost on non-full send error | confirmed | High |
| H9-B3 | `max_transaction_batch_size=0` disables auto-flush silently | confirmed | Low |
| H9-B4 | `continue_propagation` is a no-op in production | confirmed | Low |
| H10-B1 | `convert_to_rpc_tx` failure skips `ADDED_TRANSACTIONS_FAILURE` metric | confirmed | Medium |
| H10-B2 | Regex recompiled on every error response | confirmed | Low |
| H10-B3 | Version field found at wrong JSON position | suspected | Low |
| H10-B4 | RPC endpoint malformed JSON not counted in TOTAL | confirmed | Low |
| H11-B1 | Metrics double-counted after LRU cache eviction | confirmed | Medium |
| H11-B2 | `validate_class_version` runs after expensive compilation | confirmed | Low |
| H11-B3 | Storage error silently swallowed in existence check | confirmed | Medium |
| H11-B4 | Crash between file write and DB marker leaves class un-writable | suspected | Medium |
| H11-B5 | Misleading panic assertion in `record_class_size` for deprecated classes | rejected | N/A |
| H12-B1 | `is_left_descendant` panics on zero-length `PathToBottom` | confirmed | Medium |
| H12-B2 | `bisect_left`/`bisect_right` incorrect with duplicate indices | confirmed | Low |

---

## Hunter 9 — apollo_mempool_p2p

### H9-B1: Metric recorded even when broadcast fails or batch is dropped

**Verdict**: confirmed

**Rationale**: Verified in `propagator/mod.rs` lines 117–140. The function drains the queue, then calls `broadcast_message(...).or_else(...)`. The `or_else` closure either returns `Err(NetworkSendError)` (on a non-full error) or `Ok(())` (on a full-buffer drop). In both cases, `MEMPOOL_P2P_BROADCASTED_BATCH_SIZE.record(number_of_transactions_in_batch)` fires unconditionally at line 138. The metric is named "broadcasted batch size" and semantically should track only successful broadcasts.

The test provided is not fully runnable as-is (it references `drop(mock_network)` after `mock_network` has already been consumed), but the bug is unambiguously visible from code inspection alone and does not require test validation — line 138 is outside any conditional branch.

**Fix suggestion**: Gate the record call on `result.is_ok()`:
```rust
if result.is_ok() {
    MEMPOOL_P2P_BROADCASTED_BATCH_SIZE.record(number_of_transactions_in_batch);
}
result
```

---

### H9-B2: Transactions permanently lost on non-full send error

**Verdict**: confirmed

**Rationale**: Verified in `propagator/mod.rs` line 118: `self.transaction_queue.drain(..).collect()` empties the queue before the fallible `broadcast_message` call. If `broadcast_message` returns an error that is not `is_full()`, the `or_else` closure returns `Err(NetworkSendError)` at line 130, and the function returns that error — but the drained transactions are already gone. There is no re-enqueue on the error path. The `is_full()` path (drop-by-design) at least logs "Dropping the transaction batch", but the non-full error path silently loses transactions with only a "Error broadcasting transaction batch" warning.

No mechanical test is provided, but the data flow is unambiguous. The hunter's fix suggestion (re-enqueue in the error closure) is correct.

**Fix suggestion**: Collect transactions into a local `Vec`, clone them for the send, and re-enqueue on non-full errors, OR use `drain` only after a successful send.

---

### H9-B3: `max_transaction_batch_size=0` disables auto-flush silently

**Verdict**: confirmed

**Rationale**: At line 96, `add_transaction` checks `self.transaction_queue.len() == self.max_transaction_batch_size`. After pushing a transaction, `len()` is at least 1. If `max_transaction_batch_size = 0`, this condition is never true, so auto-flush never fires. The config struct `MempoolP2pConfig` in `config.rs` has no `#[validate(range(min = 1))]` attribute on `max_transaction_batch_size`. The provided test is structurally sound — it constructs a `MempoolP2pPropagator` with `max_transaction_batch_size = 0`, adds a transaction, and asserts no broadcast happened. The assertion is correct; the second assertion (that an explicit `BroadcastQueuedTransactions` request flushes it) also correctly demonstrates the bug: transactions accumulate indefinitely unless the timer fires.

The test is legitimate — zero is a valid `usize` value, the public API accepts it, and a misconfigured deployment would silently degrade transaction propagation.

**Fix suggestion**: Add `#[validate(range(min = 1))]` to `max_transaction_batch_size`, or change the auto-flush condition to `>= self.max_transaction_batch_size` (which still misbehaves at 0 but at least flushes on first add for `max_transaction_batch_size = 1`). The validate attribute is the cleaner fix.

---

### H9-B4: `continue_propagation` is a no-op in production

**Verdict**: confirmed

**Rationale**: Verified in `network_manager/swarm_trait.rs` line 133: the production `Swarm<MixedBehaviour>` implementation of `continue_propagation` is an empty stub with a `// TODO(shahak): Implement this function.` comment. The `MempoolP2pPropagator` wires up a `ContinuePropagation` request variant and forwards it to `broadcast_topic_client.continue_propagation(...)`, but that ultimately calls the empty stub. Mock-based tests pass because the mock records the call via a real mpsc sender — but production achieves nothing. This is a known incomplete feature (the TODO acknowledges it), not a hidden bug, but it means the application-layer validation signaling API is entirely inoperative. Impact is limited to gossipsub peer scoring and propagation control, which fall back to libp2p defaults.

**Fix suggestion**: Track this as a known incomplete feature and document it. The `MempoolP2pRunner` should be audited to ensure it does not make decisions based on the assumption that `continue_propagation` works.

---

## Hunter 10 — apollo_http_server

### H10-B1: `convert_to_rpc_tx` failure skips `ADDED_TRANSACTIONS_FAILURE` metric

**Verdict**: confirmed

**Rationale**: Verified in `http_server.rs` lines 196–216. `ADDED_TRANSACTIONS_TOTAL` is incremented at line 196 (before any processing). At line 212–214, `convert_to_rpc_tx` is called with `inspect_err` that only logs a debug message — no `increment_failure_metrics` call and no `ADDED_TRANSACTIONS_FAILURE` increment. The `?` returns `Err(HttpServerError::DecompressionError(...))` before reaching `add_tx_inner`, which is where `record_added_transactions` (the success/failure counter updater) is called. This breaks the invariant `TOTAL == SUCCESS + FAILURE`.

The test is not runnable as written (it references a non-existent `HttpServerConfig::new` constructor with 3 arguments and accesses `http_client.client` which may not be public), but the code path is clear and the test intent is valid.

**Fix suggestion**: Add `ADDED_TRANSACTIONS_FAILURE.increment(1)` inside the `inspect_err` closure at line 212, mirroring all other failure paths.

---

### H10-B2: Regex recompiled on every error response

**Verdict**: confirmed

**Rationale**: Verified in `errors.rs` lines 128–129. Both `Regex::new(r#"[\"``]"#)` and `Regex::new(r#"[^a-zA-Z0-9 :.,\[\]\(\)\{\}'_]"#)` are called inside `serialize_error`, which is invoked for every error response. Regex compilation involves NFA/DFA construction and is non-trivial. Under a flood of malformed requests, this amplifies CPU usage unnecessarily. The test demonstrates the performance characteristic but is not a correctness test. The bug is real but low-severity (no correctness impact, measurable performance impact only under load).

**Fix suggestion**: Use `std::sync::LazyLock<Regex>` or `once_cell::sync::Lazy<Regex>` for both regexes as module-level statics.

---

### H10-B3: Version field found at wrong JSON position

**Verdict**: suspected

**Rationale**: Verified in `http_server.rs` lines 236–261. The function strips all whitespace (including from inside string values), then calls `compact.find(marker)` where `marker = "\"version\":\""`. This finds the first occurrence of that literal substring anywhere in the compact string. The hunter's claim that a JSON string value containing the literal text `"version":"0x1"` would be found first is theoretically correct.

However, this function is only called when `serde_json::from_str::<DeprecatedGatewayTransactionV3>` has already failed (line 198). The primary question is whether a payload with a calldata string element containing `"version":"0x1"` would actually produce a different top-level version field in its JSON structure such that `serde_json` deserialization succeeds yet the version check fails — that scenario is incoherent. More likely: the deserialization fails for a different reason, and the version check just produces a misleading error code. This does affect the metric increment (wrong counter bumped) and the error message shown to callers, but only for malformed or adversarially crafted payloads.

The test relies on constructing a JSON blob where a string element contains the text `"version":"0x1"` before the real top-level `"version":"0x3"` key, which requires the attacker to control calldata content. In practice, JSON serializers always output unescaped keys without surrounding quotes in string values, so this scenario requires deliberate crafting. The impact is limited to misleading error responses for malformed inputs.

**What would make it confirmable**: A runnable test demonstrating that `validate_supported_tx_version_str` returns `Err(InvalidTransactionVersion)` for a crafted input that has `"version":"0x3"` at the top level (would be valid) but also has `"version":"0x1"` embedded in a string value earlier in the serialized representation.

---

### H10-B4: RPC endpoint malformed JSON not counted in TOTAL

**Verdict**: confirmed

**Rationale**: Verified by reading `http_server.rs`. The `add_rpc_tx` handler at line 168 uses `Json(tx): Json<RpcTransaction>` as a typed extractor. Axum evaluates extractors before the handler body runs; if `Json<RpcTransaction>` extraction fails (malformed JSON body), axum returns a 422 response without calling the handler. The `ADDED_TRANSACTIONS_TOTAL.increment(1)` at line 178 inside the handler body is never reached.

The `add_tx` handler for the deprecated endpoint accepts `tx: String` (line 188) and handles JSON parsing internally, so all parse failures do increment `TOTAL`. This is an observable asymmetry in metrics behavior between the two endpoints.

The test as written accesses `http_client.client` and `http_client.socket` which may or may not be accessible — but the test intent is valid and the behavior is straightforward to verify by reading the code.

**Fix suggestion**: Change `add_rpc_tx` to accept `body: String`, parse the JSON internally with `serde_json::from_str::<RpcTransaction>(&body)`, increment `TOTAL` first, then handle parse errors, mirroring the `add_tx` pattern.

---

## Hunter 11 — apollo_class_manager

### H11-B1: Metrics double-counted after LRU cache eviction

**Verdict**: confirmed

**Rationale**: Verified in `class_storage.rs` lines 106–138. `CachedClassStorage::set_class` returns early at line 114 only if `self.class_cached(class_id)` is true (i.e., the class is in the LRU cache). If the class was evicted from the LRU cache but is still in persistent storage, `class_cached` returns false, so the code falls through to call `self.storage.set_class(...)`. The underlying `FsClassStorage::set_class` at line 497 has its own early-exit (`if self.contains_class(class_id)? { return Ok(()); }`), which prevents a duplicate write. However, the metrics calls at lines 125–127 execute unconditionally after `self.storage.set_class(...)` returns — whether it wrote a new class or silently returned early due to the class already existing in persistent storage. This causes double-counting on every re-add of a cache-evicted class.

The test is structurally sound and exercises real code paths through the public API. It relies on an LRU cache of size 1, which is a valid `CachedClassStorageConfig` value. The test correctly demonstrates the eviction path and the re-add triggering a spurious metric.

**Fix suggestion**: Have `FsClassStorage::set_class` return a boolean indicating whether it actually wrote (or add an explicit `contains_class` check before the `storage.set_class` call in `CachedClassStorage`). Gate metrics on genuine new insertions.

---

### H11-B2: `validate_class_version` runs after expensive compilation

**Verdict**: confirmed

**Rationale**: Verified in `class_manager.rs` lines 71–113. `sierra_class` is computed at line 72 via `SierraContractClass::try_from(&class)?`. `Self::validate_class_version(&sierra_class)` is called at line 103, after both the async `self.compiler.compile(class.clone()).await` call (line 82) and the `validate_class_length` call (line 102). Since `sierra_class` is available before compilation, the version check can be moved to immediately after line 72 without any correctness impact. An adversary submitting classes with an unsupported `contract_class_version` string would force a full compiler round-trip before rejection.

The test is well-formed and uses `mockall` correctly. Setting `compiler.expect_compile().never()` then calling `add_class` with a bad-version class currently fails the expectation because `compile` IS called before `validate_class_version`. After the fix (moving `validate_class_version` to before `compile`), the test would pass.

**Fix suggestion**: Move `Self::validate_class_version(&sierra_class)?;` to line 73, immediately after `sierra_class` is computed and before the `Instant::now()` timing call.

---

### H11-B3: Storage error silently swallowed in existence check

**Verdict**: confirmed

**Rationale**: Verified in `class_manager.rs` lines 74–79. The pattern `if let Ok(Some(executable_class_hash_v2)) = self.classes.get_executable_class_hash_v2(class_hash)` silently ignores both `Err(...)` (storage failure) and `Ok(None)` (class not found). When storage returns an error, the code falls through and proceeds to compile and re-write the class — masking the error entirely. The comment in the code says "Class already exists" but only handles the `Ok(Some(...))` case; a storage error is indistinguishable from "not found" from this code's perspective.

The test mock approach is acknowledged as difficult to wire up without refactoring (since `ClassManager` is generic over a concrete `FsClassStorage` in practice). The bug is directly verifiable by code reading.

**Fix suggestion**: Replace the `if let Ok(Some(...))` with a `match ... ?` pattern that propagates the error:
```rust
match self.classes.get_executable_class_hash_v2(class_hash)? {
    Some(executable_class_hash_v2) => return Ok(ClassHashes { class_hash, executable_class_hash_v2 }),
    None => {}
}
```

---

### H11-B4: Crash between file write and DB marker leaves class permanently un-writable

**Verdict**: suspected

**Rationale**: The two-phase write in `FsClassStorage::set_class` (lines 501–502) is described correctly: files are written to the persistent path via atomic rename, then the MDBX marker is written. A crash between these two operations leaves the directory on disk but no MDBX marker. On restart, `contains_class` returns false, `write_class_atomically` is called, and `std::fs::rename` may fail with `ENOTEMPTY` if the destination directory already exists and is non-empty.

The hunter's analysis of `rename(2)` behavior on Linux is correct: POSIX specifies `ENOTEMPTY` (or `EEXIST` on some implementations) when the destination directory exists and is non-empty.

However, the scenario requires a process crash at a precise point between two operations that complete in microseconds, and the recovery impact (re-add fails) requires that the same class be submitted again after restart. This is an operational reliability issue but not a correctness issue under normal execution. The test cannot be mechanically demonstrated without process-kill simulation, as the hunter acknowledges.

**What would make it confirmable**: An integration test or runbook showing a real restart scenario where a class becomes un-writable. Alternatively, reading the `write_class_atomically` implementation to confirm that `rename` is not wrapped with an ENOTEMPTY-recovery path (confirmed: it is not — `std::fs::rename(tmp_dir, persistent_dir)?` is naked).

**Fix suggestion**: In `write_class_atomically`, check if `persistent_dir` already exists before the rename. If it exists, verify its contents are consistent with the class being written, then skip the rename. Or, as a simpler recovery: add `std::fs::remove_dir_all(&persistent_dir)` before `rename` if the rename fails with `ENOTEMPTY` (but verify file integrity first).

---

### H11-B5: Misleading panic assertion in `record_class_size` for deprecated classes

**Verdict**: rejected

**Rationale**: The hunter acknowledges that deprecated classes added via `add_deprecated_class` bypass `validate_class_length`. The claimed bug is that `record_class_size` would panic at the `u32::try_from(class_size)` cast for a deprecated class larger than 4 GB.

This is not a real bug:
1. A 4 GB deprecated class is not a realistic input — it would have to be transmitted, deserialized, and stored, all of which would fail far earlier than the metrics call.
2. The `u32` cast panic is documented with a message saying the class "should not have gotten into the system." While technically the invariant is not enforced for deprecated classes in `class_manager.rs`, the practical maximum size of any class that could reach this code is orders of magnitude below `u32::MAX`.
3. The test as written asserts `result.is_ok()`, documenting an "intentional by design" behavior — the test proves this is not a bug, it contradicts itself.

The concern about the misleading panic message is a documentation/comment quality issue, not a bug.

---

## Hunter 12 — starknet_patricia

### H12-B1: `is_left_descendant` panics on zero-length `PathToBottom`

**Verdict**: confirmed

**Rationale**: Verified in `inner_node.rs` line 173: `self.path.0 >> (self.length.0 - 1)`. `self.length.0` is a `u8`. When `self.length.0 == 0`, this computes `0u8 - 1`, which panics in debug mode (arithmetic overflow) and wraps to 255 in release mode, making `>> 255` on a `U256` evaluate to 0, returning `true` (incorrectly claiming the zero-length path is a left descendant).

`PathToBottom::new_zero()` is a public API function (line 198) that produces exactly this value. The function is used in tests (`create_tree_helper_test.rs` line 129 uses `EdgePathLength::new(0).unwrap()`, producing the same structure) and in `get_path_to_descendant` (line 130), which returns a zero-length path when the descendant equals the root — a valid scenario when a leaf is at the root level of a subtree.

The call to `is_left_descendant` in `update_edge_node` (line 315 of `create_tree_helper.rs`) receives `path_to_bottom` from an `OriginalSkeletonNode::Edge`. A zero-length edge can arise from storage or from `concat_paths`. This is a real panic risk.

The tests are legitimate — they use only public API (`PathToBottom::new_zero()`, `PathToBottom::new(...)`) to construct the offending input and call the method normally.

**Fix suggestion**: Guard the method: `if self.length.0 == 0 { return true; }` (or return an error/panic with a descriptive message). Alternatively, disallow `EdgePathLength(0)` in `PathToBottom::new` to prevent the struct from being constructed with zero length.

---

### H12-B2: `bisect_left`/`bisect_right` incorrect with duplicate indices

**Verdict**: confirmed

**Rationale**: Verified in `types.rs` lines 231–288. `SortedLeafIndices::new` calls `indices.sort()` but does NOT call `dedup()`, and a `TODO(Nimrod, 1/8/2024): Remove duplicates from the given indices.` comment explicitly acknowledges this. `bisect_left` uses `slice::binary_search`, which for duplicate elements returns `Ok(pos)` where `pos` is an arbitrary occurrence — specifically, the Rust standard library documents that for duplicates `binary_search` returns "some matching index" with no guarantee of leftmost or rightmost. This means `split_leaves` may place a duplicate in the wrong subtree.

The test is legitimate and demonstrates the bug using only public API calls: `SortedLeafIndices::new` and `split_leaves`. The scenario (two modifications to the same leaf index in one batch) is architecturally possible if a caller de-duplicates incorrectly or if the API is called defensively.

The bug is low-severity in practice because most callers would deduplicate leaf modifications before calling into the tree, but the invariant is not enforced and the TODO comment confirms it is a known gap.

**Fix suggestion**: Add `indices.dedup()` after `indices.sort()` in `SortedLeafIndices::new`, as the TODO comment intends.
