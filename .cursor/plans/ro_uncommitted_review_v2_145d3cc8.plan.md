---
name: RO Uncommitted Review v2
overview: Exhaustive trace of every RO operation in the sync flow, with exact execution ordering, determining which need uncommitted exposure and how to fix them by modifying the functions themselves.
todos:
  - id: fix-verify-parent
    content: Add last_stored_block_hash field to GenericStateSync; modify verify_parent_block_hash to use it instead of storage read
    status: pending
  - id: fix-stale-markers
    content: "Fix stale marker re-download causing MarkerMismatch: either in-memory markers, or max_stream_size >= batch_size constraint, or idempotent append"
    status: pending
isProject: false
---

# Review: RO Exposure to Uncommitted Data During Sync (v2 - Thorough)

## Architecture Context

Central sync runs in [lib.rs](crates/apollo_central_sync/src/lib.rs). The main loop (`sync_while_ok`, line 258) uses `select!` to poll 4 streams + 1 progress checker. `select!` picks ONE ready event, then `process_sync_event` (line 331) dispatches it.

**Critical fact:** Events are processed ONE AT A TIME -- no concurrent writes. But streams interleave via `select!` -- between events from the same stream, events from OTHER streams may be processed.

**How streams work:** Each stream has an OUTER loop (reads markers, decides download range) and an INNER loop (yields pre-fetched items one by one). The inner loop does NOT re-read markers. Markers are only re-read when the inner loop finishes and the outer loop restarts.

**With batching (batch_size > 1):** `commit()` increments a counter; MDBX only flushes when `counter >= batch_size`. Between flushes, data lives in the unflushed MDBX transaction. A fresh RO transaction (independent MDBX snapshot) does NOT see this unflushed data.

---

## Every RO Operation -- Traced With Exact Flow

### RO #1: `verify_parent_block_hash` -- line 688 (called from `store_block` at line 387)

**Exact flow:**

1. Block stream downloads blocks `[header_marker, up_to)` and yields them one by one
2. `select!` picks `BlockAvailable { block_number: N }`
3. `process_sync_event` -> `store_block(N)` (line 334)
4. First thing `store_block` does: `self.verify_parent_block_hash(N, &block)` (line 387)
5. Inside: `self.reader.begin_ro_txn()?.get_block_header(N-1)?` (line 699)
6. Block N-1 was written by `store_block(N-1)` in the PREVIOUS loop iteration
7. `store_block(N-1)` called `commit()` -- but with batching, this did NOT flush

**Concrete crash scenario (batch_size=100):**

- Iter 1: `store_block(0)` writes header_0, commit (counter=1). Block 0 has no parent -> OK
- Iter 2: `store_block(1)` -> `verify_parent_block_hash(1)` -> `get_block_header(0)` via RO -> **block 0 is unflushed** -> **returns None** -> **CRASH: "Missing block 0 in the storage"**

**Verdict: NEEDS FIX**

**Fix:** Add a field `last_stored_block_hash: Option<(BlockNumber, BlockHash)>` to `GenericStateSync`. After `store_block(N)` completes its write, set `self.last_stored_block_hash = Some((N, block.header.block_hash))`. Modify `verify_parent_block_hash(N+1)` to check this field first -- if it holds block N, use its hash. Fall back to storage read only for the very first block after startup (whose predecessor is from a prior committed batch).

---

### RO #2: `get_deprecated_class_definition_block_number` in `store_state_diff` -- line 451

**Exact flow:**

1. State diff stream yields `StateDiffAvailable { block_number: N }`
2. `store_state_diff(N)` runs (line 434)
3. BEFORE any writes, reads via RO: `state_reader.get_deprecated_class_definition_block_number(&class_hash)` (line 457)
4. Purpose: filter out classes already declared in previous blocks

**With stale data:** If block M < N declared class 0x123 (unflushed), this filter won't see it. Block N will try to declare 0x123 again. But the storage layer's `write_deprecated_classes()` does `get(txn, class_hash)` within the MDBX transaction, which DOES see unflushed writes from the same txn. So duplicates are caught at the storage level.

**Verdict: SAFE -- storage-layer dedup handles it. Slightly wasteful (redundant declaration attempt) but correct.**

---

### RO #3: `get_compiler_backward_compatibility_marker()` in `store_state_diff` -- line 497

**Exact flow:**

1. `store_state_diff(N)` reads this marker via RO (line 497)
2. This marker was set by `store_block(N)` at line 407 (`update_compiler_backward_compatibility_marker`)
3. `store_block` runs in the block stream; `store_state_diff` runs in the state diff stream

**Why it is safe:** The state diff stream reads committed `header_marker` (RO #6 below) to determine its upper bound. It only downloads state diffs for blocks BELOW the committed `header_marker`. For `header_marker` to be committed at value H, all block writes [0, H) must have flushed. When those block writes flushed, the `compiler_backward_compatibility_marker` flushed with them. So when `store_state_diff(N)` reads this marker, N is below the committed header_marker, and the marker correctly reflects all blocks up to that flush point.

**Verdict: SAFE -- marker is committed by the time state diffs run, because the state diff stream only advances after header batch flushes.**

---

### RO #4: `get_compiled_class_marker()` in `store_state_diff` -- line 584

**Exact flow:** After writing the state diff, reads the compiled class marker. Used only for the `STATE_SYNC_COMPILED_CLASS_MARKER` metric.

**Verdict: SAFE -- metrics only. Stale value = slightly off metric, no correctness impact.**

---

### RO #5: `get_class(&class_hash)` in `store_compiled_class` -- line 607

**Exact flow:**

1. Compiled class stream reads committed `state_marker = S` and `compiled_class_marker = M` (RO #9)
2. Stream iterates blocks [M, S), reads committed state diffs, extracts class hashes
3. Downloads compiled classes from feeder gateway
4. Yields `CompiledClassAvailable { class_hash }`
5. `store_compiled_class` does: `self.reader.begin_ro_txn()?.get_class(&class_hash)?` (line 607)
6. The Sierra class was written by `store_state_diff(X).append_classes()` for some block X < S

**Key guarantee:** For `state_marker` to be committed at value S, all state diff writes for blocks [0, S) must have flushed -- including their `append_classes()`. So the Sierra class for any class_hash from block X < S is committed.

**Verdict: SAFE -- the compiled class stream's design guarantees it only processes classes whose Sierra data is committed. No uncommitted exposure needed.**

However, if we want extra safety (defense in depth), we can cache Sierra classes in memory during `store_state_diff` and read from cache in `store_compiled_class`.

---

### RO #6: `get_state_marker()` + `get_header_marker()` in `stream_new_state_diffs` -- lines 804-806

**Exact flow:**

1. Top of state diff stream outer loop
2. Reads `state_marker` (committed) and `header_marker` (committed)
3. If `state_marker == header_marker` -> sleeps (no new blocks to process)
4. Otherwise downloads state diffs `[state_marker, min(header_marker, max_stream_size))`
5. Inner loop yields state diff events one by one
6. After inner loop finishes, outer loop re-reads markers

**Problem scenario (batch_size=100, max_stream_size=32):**

- Header batch flushes: `header_marker = 100` committed
- State diff stream downloads [0, 32) and yields them
- All 32 state diffs processed (64 commits, batch NOT flushed, counter=64)
- Outer loop re-reads: `state_marker` committed = 0 (still! batch didn't flush)
- Stream tries to download [0, 32) AGAIN
- `store_state_diff(0)` -> `append_state_diff(0)` -> RW txn sees marker = 32 (unflushed) -> **MarkerMismatch { expected: 32, found: 0 } -> CRASH**

**Verdict: NEEDS FIX -- stale marker causes re-processing and MarkerMismatch crash if `max_stream_size < batch_size`.**

**Fix options:**

- **(a)** Keep in-memory `current_state_marker` on `GenericStateSync`. After each `store_state_diff`, update it. Pass to stream generator.
- **(b)** Ensure `max_stream_size >= batch_size` so the batch always flushes within one stream cycle.

---

### RO #7: `get_header_marker()` in `stream_new_blocks` -- line 756

**Same pattern as RO #6.** After processing a download batch of blocks, the outer loop re-reads the committed header_marker. If the batch hasn't flushed, sees stale marker, re-downloads, and `append_header` fails with MarkerMismatch.

**Verdict: NEEDS FIX -- same stale marker issue.**

---

### RO #8: `get_state_marker()` in `stream_new_blocks` -- line 765

**Flow:** Only checked when `header_marker == central_block_marker` (fully caught up). At that point, all data is committed.

**Verdict: SAFE.**

---

### RO #9: Markers in `stream_new_compiled_classes` -- lines 892-895

**Same pattern as RO #6/#7.** After processing compiled classes, re-reads stale markers. If `max_stream_size < batch_size`, could cause MarkerMismatch on re-processing.

**Verdict: NEEDS FIX -- same stale marker issue.**

---

### RO #10: `get_state_diff(from)` in `stream_new_compiled_classes` -- line 898

**Flow:** Iterates `from < state_marker` (committed), reads state diffs to skip blocks without classes.

**Verdict: SAFE -- all data below committed state_marker is committed.**

---

### RO #11: `get_header_marker()` in `stream_new_base_layer_block` -- line 961

**Flow:** Compares header marker to base layer tip. If stale, just waits longer.

**Verdict: SAFE -- comparison only, no write attempted.**

---

### RO #12: All markers in `check_sync_progress` -- lines 993-1010

**Flow:** Periodically reads markers to detect stuck sync. If no progress, yields `NoProgress` -> sync restarts.

**With stale data:** May see "no progress" when progress actually happened but wasn't flushed. Causes unnecessary sync restart.

**Verdict: SAFE -- worst case is an unnecessary restart, not data corruption.**

---

### RO #13: `get_compiled_class_marker()` in `store_compiled_class` -- line 638

**Flow:** After writing CASM, reads marker for metrics.

**Verdict: SAFE -- metrics only.**

---

### RO #14: `get_state_diff(bn)` in `CentralSource::stream_compiled_classes` -- central.rs line 204

**Flow:** Reads state diffs for blocks `[initial, up_to)` where both bounds come from committed markers.

**Verdict: SAFE -- behind committed marker.**

---

### RO #15: `download_class_if_necessary` -- state_update_stream.rs lines 359-384

**Flow:** Checks if class already exists in storage before downloading from feeder. Reads at committed `state_marker`.

**Verdict: SAFE -- reads committed data. If class is unflushed, it re-downloads (redundant but correct).**

---

### RO #16: `sync_pending_data` -- pending_sync.rs lines 33-47

**Flow:** Reads header marker and block header. Only runs when `header_marker == central_block_marker` AND `state_marker == header_marker`.

**Verdict: SAFE -- fully synced, all committed.**

---

### RO #17-#20: P2P sync (`crates/apollo_p2p_sync/src/client/`)

P2P sync uses `BlockNumberLimit` to constrain each stream:

- **State diff** (state_diff.rs:62): reads `get_block_header(N)` where N < committed `header_marker`. SAFE.
- **Transaction** (transaction.rs:62): same. SAFE.
- **Class** (class.rs:96): reads `get_state_diff(N)` where N < committed `state_marker`. SAFE.
- **Stream builder** (block_data_stream_builder.rs:121): reads committed markers for limits. Has same stale marker re-download issue as central sync.

---

## Final Verdict

### Operations requiring a fix (2 distinct issues):

**Issue 1: `verify_parent_block_hash` crashes (RO #1)**

- Direct crash: reads unflushed block N-1 header -> None -> error
- **Fix:** Keep `last_stored_block_hash` in memory on `GenericStateSync`. Modify `verify_parent_block_hash` to accept it as parameter or read from the struct field. No storage read needed.

**Issue 2: Stale marker re-download causes MarkerMismatch (RO #6, #7, #9)**

- Indirect crash: stream re-reads stale committed marker, re-attempts to write already-written data -> MarkerMismatch
- **Fix (option a):** Keep in-memory markers (`current_header_marker`, `current_state_marker`, `current_casm_marker`) on `GenericStateSync`. Update after each `store_*`. Pass to streams.
- **Fix (option b):** Guarantee `max_stream_size >= batch_size` so batches always flush within one stream cycle.
- **Fix (option c):** Make `append_header` / `append_state_diff` idempotent -- if marker already advanced past this block, skip instead of error.

### Operations that are SAFE (all others):

- **RO #2** (deprecated class filter): storage layer dedup handles duplicates
- **RO #3** (compiler compat marker): committed by time state diffs run  
- **RO #4, #11, #13** (metrics markers): metrics only
- **RO #5** (get_class for compiled class): compiled class stream guarantees class is committed
- **RO #8** (state marker in blocks): only runs when fully synced
- **RO #10** (state diff in compiled stream): behind committed marker
- **RO #12** (progress check): monitoring only
- **RO #14** (state diff in source): behind committed marker
- **RO #15** (download class if necessary): reads committed data
- **RO #16** (pending sync): fully synced
- **RO #17-20** (P2P data reads): behind committed markers

### How the queue-based POC handled both issues:

1. **Parent hash:** POC kept block data in its own batch buffer. Verification used in-memory data, not storage.
2. **Stale markers:** POC controlled flush. Streams only re-read markers AFTER POC flushed, so markers were always current.
3. **Sierra class:** POC deferred reads to after flush (COLLECT -> COMMIT -> READ). In our case this is actually safe anyway (proven above), but the POC was even more explicit about it.