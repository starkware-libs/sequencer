# Supervisor 2 Validation Report

Bugs from Hunter 5 (apollo_state_sync), Hunter 6 (blockifier), Hunter 7 (apollo_storage), Hunter 8 (apollo_l1_gas_price).

---

## Summary Table

| Bug ID | Title | Verdict | Severity |
|--------|-------|---------|----------|
| H5-B1 | `is_cairo_1_class_declared_at` / `is_class_declared_at` skip `verify_synced_up_to` | **confirmed** | High |
| H5-B2 | `get_nonce_at` maps `None` nonce to `ContractNotFound` wrong error | **rejected** | — |
| H5-B3 | `get_latest_block_header` silently returns `None` for missing header | **suspected** | Low |
| H6-B1 | DA gas fee-balance discount applied unconditionally even with zero storage updates | **confirmed** | Medium |
| H6-B2 | Events of nested failed inner calls not cleared on revert (DFS filter bug) | **suspected** | High |
| H6-B3 | `validate_reads` panics on keys not in versioned storage | **rejected** | — |
| H6-B4 | Alias counter written spuriously when `next_free_alias == None` and no aliases allocated | **confirmed** | Medium |
| H6-B5 | Unnecessary state read when cost-exceeds-bounds check already failed | **rejected** | — |
| H7-B1 | `iter_events` silently drops `to_block_number` for contract-address path | **confirmed** | High |
| H7-B2 | `scan_at_block` arithmetic overflow when `block_target == u64::MAX` | **confirmed** | Medium |
| H7-B3 | Inverted condition string in `unreachable!` messages | **confirmed** | Low |
| H7-B4 | File offsets not updated when appending empty block bodies | **rejected** | — |
| H7-B5 | Misleading `expect` panic in `EventIterByContractAddress::next` | **rejected** | — |
| H8-B1 | Integer underflow in `fetch_rate` when `timestamp < lag_interval_seconds` | **confirmed** | High |
| H8-B2 | Integer underflow in `fetch_rate` when `quantized_timestamp == 0` | **confirmed** | Medium |
| H8-B3 | Dead assertion in `gas_price_provider_adding_blocks` test | **confirmed** | Low |
| H8-B4 | u64 overflow in stale-price check | **confirmed** | Medium |
| H8-B5 | `LATEST_SCRAPED_BLOCK` metric is off by one | **confirmed** | Low |

---

## Hunter 5 — apollo_state_sync

### H5-B1: `is_cairo_1_class_declared_at` / `is_class_declared_at` skip `verify_synced_up_to`

**Verdict: confirmed**

**Rationale**: Traced the code at `lib.rs:299–337`. `is_cairo_1_class_declared_at` opens a transaction and immediately calls `get_class_definition_block_number`, then compares the result against `block_number`. There is no call to `verify_synced_up_to`. Contrast with `get_nonce_at` (line 251), `get_class_hash_at` (line 272), and `get_storage_at` (not shown but same pattern) which all begin with `verify_synced_up_to(&txn, block_number)?`. The consequence is exactly as described: when a node is synced to block N and a caller queries block M > N, the function returns `Ok(false)` instead of `Err(StateSyncError::BlockNotFound(M))`.

`is_class_declared_at` inherits the bug by delegating to `is_cairo_1_class_declared_at` and performing its own unguarded deprecated-class lookup.

The proposed test uses only public API (`setup()`, `handle_request`) and legitimate storage state (empty storage or a single well-formed block). It correctly describes the observable mismatch.

**Fix suggestion**: Add `verify_synced_up_to(&txn, block_number)?` at the start of `is_cairo_1_class_declared_at`, before accessing the state reader. For `is_class_declared_at`, add the same guard before the Cairo-0 deprecated-class lookup (the delegation to `is_cairo_1_class_declared_at` already returns early if that guard fires, but the deprecated-class path still needs its own guard or a shared transaction with the guard applied once).

---

### H5-B2: `get_nonce_at` maps `None` nonce to `ContractNotFound` wrong error

**Verdict: rejected**

**Rationale**: The hunter correctly identifies that line 260 uses `.ok_or(StateSyncError::ContractNotFound(...))` after `verify_contract_deployed` has succeeded. However, the hunter's own analysis (and the proposed test) confirms the path is unreachable in practice: `write_deployed_contracts` in `apollo_storage` always writes `Nonce::default()` for newly-deployed contracts, so storage never returns `None` for a deployed contract's nonce. The proposed test explicitly reaches the conclusion "this test will actually pass with `Ok(Nonce::default())`" and notes that "the write path compensates for the read-path bug."

A code smell or future-fragility concern does not qualify as a confirmed bug under the validation criteria. The current system is self-consistent and no real user can observe the wrong error. The only path to the wrong error requires DB corruption or intentional internal API abuse, which is not normal usage. The fix (using `.unwrap_or_default()`) would be a good defensive cleanup, but this is a code quality issue, not a bug.

**What would make it confirmable**: A demonstrated scenario via public APIs where a deployed contract's nonce returns `None` from storage, producing the wrong error observable to a caller. No such scenario exists with the current write path.

---

### H5-B3: `get_latest_block_header` silently returns `None` if header is missing for a synced block

**Verdict: suspected**

**Rationale**: The code at lines 289–297 is confirmed: `get_latest_block_header` calls `latest_synced_block` (which considers state and body markers only, not the header marker), then calls `txn.get_block_header(block_number)` and returns whatever it finds, including `None`. If the block's header is somehow missing, the function returns `Ok(None)` while `get_latest_block_number` returns `Ok(Some(N))` — an observable inconsistency.

The design comment ("sync always writes headers before other data") is stated in the code but not enforced by any assertion. However, the hunter themselves states this is "hard to trigger in a well-tested runtime" and "no simpler public-API reproduction exists" — the only path requires internal storage manipulation to produce header-less blocks. The condition cannot be triggered via normal public API usage, so it cannot be demonstrated as a real-user-observable bug.

The bug is architecturally plausible and the fix (returning an error instead of silently propagating `None`) would make the code more robust, but it remains undemonstrated.

**What would make it confirmable**: A test that produces the inconsistency through a plausible code path (e.g., a race condition between writer threads, or a shutdown during header write), not intentional DB manipulation.

---

## Hunter 6 — blockifier

### H6-B1: DA gas fee-balance discount applied unconditionally even with zero storage updates

**Verdict: confirmed**

**Rationale**: Traced `get_da_gas_cost` at `fee/gas_usage.rs:63–68`. Lines 64–65 unconditionally compute the fee-balance discount and add it to the running `discount` regardless of `state_changes_count.n_storage_updates`. The comment "Up to balance of 8*(10**10) ETH" does not gate the discount on storage updates being non-zero.

The existing test at line 231 of `gas_usage_test.rs` implicitly masks the issue by testing with `StateChangesCount::default()` (all zeros), where `naive_cost = 0` and `saturating_sub` clamps the result to 0 regardless. But with `n_modified_contracts: 1, n_storage_updates: 0`, the naive_cost is non-zero (2 * SHARP_GAS_PER_DA_WORD) and the fee-balance discount reduces it below the value that should be produced.

The hunter's proposed test uses only public API (`get_da_gas_cost`) with a valid `StateChangesCount`. The test correctly demonstrates that the discount is applied when no storage updates are present, and the calculated `current_cost < correct_cost` assertion would hold.

**Fix suggestion**: Gate the fee-balance discount on `n_storage_updates > 0`:
```rust
if state_changes_count.n_storage_updates > 0 {
    let fee_balance_value_cost = eth_gas_constants::get_calldata_word_cost(12);
    discount += eth_gas_constants::GAS_PER_MEMORY_WORD - fee_balance_value_cost;
}
```

---

### H6-B2: Events of nested failed inner calls not cleared on revert

**Verdict: suspected**

**Rationale**: The code at `syscall_base.rs:467–472` is confirmed: the DFS filter `!call_info.execution.failed` skips failed subcalls when clearing events. The comment states "The events and l2_to_l1_messages of the failed calls were already cleared." The hunter's scenario — a chain C1 (fails) → C2 (fails) → C3 (fails) where C2 emitted events before calling C3 — would leave C2's events un-cleared.

However, the hunter explicitly concedes the bug "is hard to reproduce mechanically without a contract that emits events before making an inner call where the inner call itself fails by calling another failing contract." No executable test is provided; the "Conceptual Test" only describes what to look for. There is no evidence the described scenario is actually reachable: in practice, when a Cairo contract calls another and that call fails, the execution framework may clear the caller's events at the lower level before returning failure upward. Without a concrete test that triggers the event-survival condition, this remains plausible but undemonstrated.

**What would make it confirmable**: An integration test using real Sierra contracts (or a carefully constructed `CallInfo` tree passed through the clearing code) that shows events surviving in C2 after a C1 revert, observable via the returned `CallInfo` structure.

---

### H6-B3: `validate_reads` panics on keys not in versioned storage

**Verdict: rejected**

**Rationale**: The hunter claims that `validate_reads` can panic via `.expect(READ_ERR)` at line 89 if a read-set key was never accessed through the proxy. However, tracing the actual call flow invalidates this:

1. `validate_reads` is called to compare a transaction's read set against the versioned state.
2. The read set is populated during the transaction's execution via `VersionedStateProxy`, which calls `get_storage_at`, `get_nonce_at`, etc.
3. Every read through `VersionedStateProxy` either finds the value in `writes` (via `storage.read`) or fetches from `initial_state` and calls `set_initial_value` before returning (see `versioned_state.rs:342–348`).
4. Therefore, every key that appears in the read set was necessarily placed into `cached_initial_values` (or has a write at or before `tx_index`) during execution. The `expect` cannot panic on a legitimately-produced read set.

The proposed test manufactures a `StateMaps` read set by hand with a key that was never accessed through the proxy — this is not how the system produces read sets. The hunter describes this directly: "Build a reads map with a storage key that was NEVER fetched through the proxy." This is an artificial state that cannot arise in normal concurrent execution. The `#[should_panic]` test validates the manufacturing of the failure condition, not a real-world reachable path.

**Rejection basis**: The test reaches into internals to construct an impossible state. In normal operation, the read set is always produced by `VersionedStateProxy`, which guarantees `set_initial_value` is called for every read. The `.expect()` is a correct defensive assertion, not a bug.

---

### H6-B4: Alias counter written spuriously when `next_free_alias == None` and no aliases allocated

**Verdict: confirmed**

**Rationale**: Traced `stateful_compression.rs:145–157`. The `None` branch of `finalize_updates` unconditionally calls `set_alias_in_storage(ALIAS_COUNTER_STORAGE_KEY, INITIAL_AVAILABLE_ALIAS)` regardless of whether `is_alias_inserted` is true. The `Some` branch correctly gates the write on `is_alias_inserted`.

The `None` branch executes when the stored counter is zero (first-ever call). With the code as written, even if all contract addresses and storage keys processed by `insert_alias` were below `MIN_VALUE_FOR_ALIAS_ALLOC` (or already aliased), the counter slot is written with `INITIAL_AVAILABLE_ALIAS` (0x80). This is a spurious state write that:
1. Incorrectly marks the alias contract as initialized.
2. Adds a write to the state diff, affecting DA gas costs.
3. After the write, subsequent calls see `stored_counter != 0` and skip the `None` branch, masking the bug.

The proposed test uses only public API (`CachedState`, `allocate_aliases_in_storage`, `get_storage_at`) with a valid small-address, small-key state diff that genuinely should not trigger aliasing.

**Fix suggestion**: Mirror the `Some` branch in the `None` case:
```rust
None => {
    if self.is_alias_inserted {
        self.set_alias_in_storage(ALIAS_COUNTER_STORAGE_KEY, INITIAL_AVAILABLE_ALIAS)?;
    }
}
```

---

### H6-B5: Unnecessary state read when cost-exceeds-bounds check already failed

**Verdict: rejected**

**Rationale**: The code at `fee_checks.rs:295–317` is as described — both checks are called eagerly and the `for` loop picks the first error. This is a performance/design concern, not a correctness bug. The hunter acknowledges: "the correctness is preserved (the first error in the `for` loop is returned)." No incorrect result is ever returned to callers. The "spurious state read mutating CachedState cache" is a performance cost measured in a single cache insert, not observable program misbehavior. The comment "If the above check passes, the pre-execution balance covers the actual cost for sure" describes the logical ordering of checks, not a requirement to short-circuit. No test is provided (only pseudocode). This does not meet the bar of a bug.

---

## Hunter 7 — apollo_storage

### H7-B1: `iter_events` drops `to_block_number` for contract-address path

**Verdict: confirmed**

**Rationale**: Traced `body/events.rs:117–130`. When `optional_address` is `Some(address)`, the function returns `EventIter::ByContractAddress(self.iter_events_by_contract_address((address, event_index))?)`. The `to_block_number` parameter is not passed to `iter_events_by_contract_address` and `EventIterByContractAddress` has no `to_block_number` field. The iteration will continue past `to_block_number`, returning events from later blocks when a contract-address filter is specified.

The proposed test uses only public storage API (`get_test_storage`, `append_header`, `append_body`, `iter_events`) with legitimate block/event data. The assertion that all returned events have block number equal to `bn0` is correct and would fail because the iterator crosses the boundary.

**Fix suggestion**: Add a `to_block_number: BlockNumber` field to `EventIterByContractAddress` and check `tx_index.0 > to_block_number` in `EventIterByContractAddress::next` before processing each entry, returning `Ok(None)` when the limit is exceeded.

---

### H7-B2: `scan_at_block` arithmetic overflow when `block_target == u64::MAX`

**Verdict: confirmed**

**Rationale**: Traced `state/mod.rs:255`. `let first_irrelevant_block = BlockNumber(block_target.0 + 1)` uses plain `+` on `u64`. When `block_target.0 == u64::MAX`, this overflows. In debug builds: panic. In release builds: wraps to `BlockNumber(0)`, causing the cursor seek `lower_bound(&(current, BlockNumber(0)))` to find entries from the beginning of time rather than past the target block, producing wrong results.

Note: The function also has a secondary bound at line 273 (`BlockNumber(u64::from(u32::MAX))`) used for key-advance seeks, which would not overflow since it uses a fixed constant. But the first overflow at line 255 occurs before any of that logic.

The proposed test uses only public API (`get_test_storage`, `scan_contract_class_hashes_in_range`) with `BlockNumber(u64::MAX)`. It would panic in debug mode, confirming the bug.

**Fix suggestion**: Use `block_target.next()` (which returns `Option<BlockNumber>`) or `block_target.0.checked_add(1)` and handle the `None`/overflow case (e.g., return an empty result since no block can be greater than `u64::MAX`).

---

### H7-B3: Inverted condition string in `unreachable!` messages

**Verdict: confirmed**

**Rationale**: Traced `header.rs:256–274` and `header.rs:404–410`. In `get_starknet_version` (line 256), the guard `if block_number >= self.get_header_marker() { return Ok(None); }` means execution continues only when `block_number < header_marker`. The `unreachable!` message at line 271 says "Since block_number >= self.get_header_marker()" — which is the opposite of the actual invariant. Same inversion at line 407 in `revert_header`.

This is a documentation-in-code error. While not a runtime correctness bug (the `unreachable!` itself is correctly placed — the branch is indeed unreachable in a consistent DB), the message will actively mislead a developer reading a panic traceback during an incident. Confirmed as a real defect in diagnostic accuracy.

**Fix suggestion**: Change both `unreachable!` messages to read "Since block_number < self.get_header_marker(), ..." to match the actual code invariant.

---

### H7-B4: File offsets not updated when appending empty block bodies

**Verdict: rejected**

**Rationale**: The hunter's own analysis concludes "The bug does not cause visible data corruption in the current single-writer model" and that the `usize` subtraction at `len() - 1` is safe in practice (the loop body never executes when the collection is empty, so the subtraction is never reached). The written justification explicitly states "No test is needed for the overflow scenario." The hunter identifies this as a "latent correctness issue" contingent on a TODO comment being ignored — this is a speculative future risk, not a current bug. No incorrect behavior is demonstrated under normal usage.

---

### H7-B5: Misleading `expect` panic in `EventIterByContractAddress::next`

**Verdict: rejected**

**Rationale**: The hunter's own extended analysis concludes: "After careful analysis: the panic in `next()` at line 208 IS reachable when the initial events_queue is empty... AND next_entry_in_event_table is Some, but the cursor's next transaction has NO ca1 events. This is impossible by the write_events invariant." The test placeholder is `assert!(true)` — it does not demonstrate the bug. This is correctly self-identified as safe given the current storage invariant. The `expect()` message is a code style concern, not a bug.

---

## Hunter 8 — apollo_l1_gas_price

### H8-B1: Integer underflow in `fetch_rate` when `timestamp < lag_interval_seconds`

**Verdict: confirmed**

**Rationale**: Traced `exchange_rate_oracle.rs:218`. `(timestamp - self.config.lag_interval_seconds)` is plain `u64` subtraction with no bounds check. When `timestamp < lag_interval_seconds`, this underflows: panic in debug, silent wrap to a huge value in release. The wrap causes the oracle to seek a nonsensical quantized timestamp in the cache, miss, and then attempt HTTP queries against a computed future timestamp — propagating `AllUrlsFailedError` or `QueryNotReadyError` rather than a meaningful diagnostic.

The proposed test uses a real `ExchangeRateOracleClient` with a valid config and a timestamp smaller than the lag interval. It correctly uses `#[should_panic]` in debug mode to demonstrate the panic.

**Fix suggestion**: Use `timestamp.checked_sub(self.config.lag_interval_seconds).ok_or(ExchangeRateOracleClientError::QueryNotReadyError(timestamp))?` before the `checked_div`.

---

### H8-B2: Integer underflow in `fetch_rate` when `quantized_timestamp == 0`

**Verdict: confirmed**

**Rationale**: Traced `exchange_rate_oracle.rs:237`. `cache.get(&(quantized_timestamp - NUMBER_OF_TIMESTAMPS_BACK))` subtracts 1 from `quantized_timestamp` without guarding against zero. `quantized_timestamp` is zero when `(timestamp - lag) / lag == 0`, i.e., when `timestamp` is in `[lag, 2*lag - 1]`. The subtraction `0u64 - 1` panics in debug mode, wraps to `u64::MAX` in release mode (causing a cache miss on a nonsensical key), and the code falls through to `QueryNotReadyError` instead of correctly using any available cached rate.

The bug is independent of Bug 1 (which triggers before this point). The proposed test using `mockito` with a delay is a legitimate way to trigger the "query not yet resolved" branch that leads to the underflow.

**Fix suggestion**: Guard the subtraction:
```rust
if quantized_timestamp > 0 {
    if let Some(rate) = cache.get(&(quantized_timestamp - NUMBER_OF_TIMESTAMPS_BACK)) {
        return Ok(*rate);
    }
}
```

---

### H8-B3: Dead assertion in `gas_price_provider_adding_blocks` test

**Verdict: confirmed**

**Rationale**: Traced `l1_gas_price_provider_test.rs:100`. The line is:
```rust
matches!(ret, Result::Err(L1GasPriceProviderError::MissingDataError { .. }));
```
`matches!` returns a `bool`; without `assert!`, the result is discarded and the test always passes regardless of `ret`. This is confirmed by direct code inspection.

The hunter also correctly identifies a secondary issue: with the default `storage_limit` (much larger than the 7 blocks added), no eviction occurs, so the `MissingDataError` path is never actually triggered even if the assertion were live. Both defects are confirmed.

**Fix suggestion**: Replace with `assert!(matches!(ret, Result::Err(L1GasPriceProviderError::MissingDataError { .. })))` and set `storage_limit` in the test config to match `number_of_blocks_for_mean` (or one less) to trigger actual eviction.

---

### H8-B4: u64 overflow in stale-price check

**Verdict: confirmed**

**Rationale**: Traced `l1_gas_price_provider.rs:134`. `*last_timestamp + self.config.max_time_gap_seconds` is plain `u64` addition. With `last_timestamp` near `u64::MAX` and any non-zero `max_time_gap_seconds`, the addition overflows: panic in debug, silent wrap to a small value in release. The wrap means the staleness threshold becomes a small number, so `timestamp.0 > small_threshold` evaluates `true` for almost any reasonable current timestamp — but only after the stale check incorrectly passes for values near the wrap point. In release mode the staleness guard is bypassed when it should fire.

The proposed test uses only public API with synthetic but structurally valid `GasPriceData`. The assertion that `StaleL1GasPricesError` is returned would fail in release mode, demonstrating the bypass.

**Fix suggestion**: Use `last_timestamp.saturating_add(self.config.max_time_gap_seconds)` to cap at `u64::MAX` rather than wrapping.

---

### H8-B5: `LATEST_SCRAPED_BLOCK` metric is off by one

**Verdict: confirmed**

**Rationale**: Traced `l1_gas_price_scraper.rs:101–113` and `135–154`. Inside `update_prices`, `*block_number` is incremented at line 154 after each successful `add_price_info`. When `update_prices` returns (either because there are no more blocks or on error), `block_number` holds the next-to-scrape value. The metric at line 113 (`L1_GAS_PRICE_SCRAPER_LATEST_SCRAPED_BLOCK.set_lossy(block_number)`) records this post-incremented value. If blocks 0–9 were scraped and block 10 does not exist, the metric records 10, but 9 was the last successfully scraped block. On error during block 5 after scraping 0–4, the metric records 5 (a failed block) as "latest scraped."

This is confirmed by code trace alone (no mechanical test needed; the metric API would require global registration). The invariant violation is unambiguous.

**Fix suggestion**: Record `block_number.saturating_sub(1)` after a successful `update_prices`, or track `last_scraped_block` as a separate variable updated inside `update_prices` before the increment.
