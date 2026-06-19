# Bug Hunter #5 Findings — apollo_state_sync

Crate examined: `/home/user/sequencer/crates/apollo_state_sync/src/`

---

## Bug 1: `is_cairo_1_class_declared_at` and `is_class_declared_at` do not verify sync horizon

**File**: `/home/user/sequencer/crates/apollo_state_sync/src/lib.rs`, lines 299–337

**Description**:
`is_cairo_1_class_declared_at` and `is_class_declared_at` accept a `block_number` parameter but never call `verify_synced_up_to`. This means they can silently return `false` for a block that the node has not yet synced, instead of returning `Err(BlockNotFound(block_number))`.

Every other state-reading method in the same `impl StateSync` block (`get_storage_at`, `get_nonce_at`, `get_class_hash_at`) calls `verify_synced_up_to` first. The trait docs in `communication.rs` document these functions as "Returns whether the given class was declared at the given block or before it", which callers naturally interpret as a definitive answer — not a "we don't know yet, but we'll say false" answer.

**Root Cause**:
Both functions were apparently written without the guard that all peer methods have. The consequence:

- Node is synced to block 50.
- Class X is declared at block 100 on the actual chain.
- Caller asks `is_cairo_1_class_declared_at(BlockNumber(100), X)`.
- Expected: `Err(StateSyncError::BlockNotFound(100))` — we haven't seen block 100 yet.
- Actual: `Ok(false)` — storage sees no class declaration before block 101, concludes the class doesn't exist, and silently lies to the caller.

For `is_class_declared_at`, the exact same defect applies because it delegates to `is_cairo_1_class_declared_at` and also performs its own unguarded deprecated-class lookup.

**Test**:
```rust
// Add to crates/apollo_state_sync/src/test.rs (follows the existing test conventions)

#[tokio::test]
async fn test_is_class_declared_at_returns_block_not_found_when_not_synced() {
    // Setup: empty storage (no blocks synced at all).
    let (mut state_sync, mut storage_writer) = setup();

    let mut rng = get_rng();
    let class_hash = ClassHash::get_test_instance(&mut rng);

    // We are asking about block 10, but the storage is empty (synced to nothing).
    // The function should return BlockNotFound(10) to be consistent with get_storage_at, etc.
    let block_number = BlockNumber(10);

    // is_cairo_1_class_declared_at — does NOT call verify_synced_up_to, so it returns Ok(false)
    // instead of Err(BlockNotFound(10)).
    let response = state_sync
        .handle_request(StateSyncRequest::IsCairo1ClassDeclaredAt(block_number, class_hash))
        .await;
    let StateSyncResponse::IsCairo1ClassDeclaredAt(result) = response else {
        panic!("Unexpected response variant");
    };
    // BUG: this assertion currently FAILS because result is Ok(false), not BlockNotFound.
    assert_eq!(result, Err(StateSyncError::BlockNotFound(block_number)));

    // is_class_declared_at has the same defect.
    let response = state_sync
        .handle_request(StateSyncRequest::IsClassDeclaredAt(block_number, class_hash))
        .await;
    let StateSyncResponse::IsClassDeclaredAt(result) = response else {
        panic!("Unexpected response variant");
    };
    assert_eq!(result, Err(StateSyncError::BlockNotFound(block_number)));
}

#[tokio::test]
async fn test_is_class_declared_at_false_positive_when_class_not_yet_synced() {
    // Setup: node synced to block 5; a class declared at block 10 does not appear in storage.
    let (mut state_sync, mut storage_writer) = setup();

    let mut rng = get_rng();
    let class_hash = ClassHash::get_test_instance(&mut rng);
    let header = BlockHeader::default(); // block 0

    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(header.block_header_without_hash.block_number, &header)
        .unwrap()
        .append_state_diff(header.block_header_without_hash.block_number, ThinStateDiff::default())
        .unwrap()
        .append_body(header.block_header_without_hash.block_number, Default::default())
        .unwrap()
        .commit()
        .unwrap();

    // Now call is_cairo_1_class_declared_at asking about block 100 (not synced).
    // The class does not exist in storage at all.
    // Expected: Err(BlockNotFound(100)) — consistent with all other read methods.
    // Actual: Ok(false) — lies to the caller saying the class wasn't declared.
    let response = state_sync
        .handle_request(StateSyncRequest::IsCairo1ClassDeclaredAt(BlockNumber(100), class_hash))
        .await;
    let StateSyncResponse::IsCairo1ClassDeclaredAt(result) = response else {
        panic!("Unexpected response variant");
    };
    // This assertion currently FAILS (result is Ok(false)).
    assert_eq!(result, Err(StateSyncError::BlockNotFound(BlockNumber(100))));
}
```

**How to verify**:
```
SEED=0 cargo test -p apollo_state_sync test_is_class_declared_at_returns_block_not_found_when_not_synced
SEED=0 cargo test -p apollo_state_sync test_is_class_declared_at_false_positive_when_class_not_yet_synced
```
Both tests will fail with a mismatch between `Ok(false)` and the expected `Err(BlockNotFound(_))`, demonstrating the bug. The fix is to add `verify_synced_up_to` calls at the start of both functions, mirroring `get_storage_at`.

---

## Bug 2: `get_nonce_at` returns `ContractNotFound` when storage returns `None` for a deployed contract's nonce

**File**: `/home/user/sequencer/crates/apollo_state_sync/src/lib.rs`, lines 256–262

**Description**:
In `StateSync::get_nonce_at`, after `verify_contract_deployed` has confirmed the contract is deployed, the code calls `state_reader.get_nonce_at(...)?.ok_or(StateSyncError::ContractNotFound(contract_address))`. If storage returns `None` for the nonce, the caller gets `Err(ContractNotFound(addr))` — yet the contract was just confirmed to exist.

This is a **semantic bug**: the wrong error variant is returned. `ContractNotFound` implies the contract was never deployed, but the contract exists; we simply have no nonce record.

In practice the storage layer always writes a `Nonce::default()` for newly-deployed contracts (see `write_deployed_contracts` in `apollo_storage/src/state/mod.rs`, lines 868–876), so a well-formed DB will never produce `None` here. However, the defensive path is still wrong: in edge cases (storage corruption, or if the storage guarantee is later relaxed), callers relying on error variant matching would observe `ContractNotFound` for a contract they can prove exists.

**Root Cause**:
The `.ok_or(StateSyncError::ContractNotFound(...))` was likely copy-pasted from `get_class_hash_at` (line 276–278), where `None` genuinely does mean the contract is not deployed. For `get_nonce_at`, a deployed contract with no explicit nonce entry should return `Nonce::default()`, not an error.

**Comparison**:
```rust
// get_class_hash_at — correct: None means not deployed
let class_hash = state_reader
    .get_class_hash_at(state_number, &contract_address)?
    .ok_or(StateSyncError::ContractNotFound(contract_address))?; // makes sense

// get_nonce_at — wrong: contract IS deployed (verified above), None means nonce is default 0
let res = state_reader
    .get_nonce_at(state_number, &contract_address)?
    .ok_or(StateSyncError::ContractNotFound(contract_address))?; // wrong error variant
```

**Test**:
This bug is hard to reproduce with a well-formed storage because the write path guards against it. To demonstrate the semantic issue we write a state diff that deploys a contract but intentionally omit the nonce entry (which is only possible by calling internal storage APIs directly). Instead, the test below demonstrates that the error surfaced is the wrong kind:

```rust
// A justification test — demonstrates the semantic mismatch in isolation.
// Add to crates/apollo_state_sync/src/test.rs

#[tokio::test]
async fn test_get_nonce_at_wrong_error_when_nonce_missing() {
    // Deploy a contract at block 0 via a state diff that has the address in
    // deployed_contracts but does NOT include it in nonces.
    // The storage layer (write_deployed_contracts) will insert Nonce::default(),
    // so in a healthy DB this path is not reachable. But if it were, the error
    // returned is ContractNotFound, not an appropriate "missing nonce" or "storage error".
    //
    // This test documents the design contract violation: after verify_contract_deployed
    // succeeds, the .ok_or(ContractNotFound) on line 260 of lib.rs is semantically wrong.
    // The correct return for a deployed contract with no nonce record is Nonce::default().

    let (mut state_sync, mut storage_writer) = setup();

    let mut rng = get_rng();
    let address = ContractAddress::from(rng.next_u64());

    // Deploy the contract but supply NO nonce in the state diff.
    let mut diff = ThinStateDiff::default();
    diff.deployed_contracts.insert(address, ClassHash::default());
    // Note: diff.nonces is intentionally empty.

    let header = BlockHeader::default();
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(header.block_header_without_hash.block_number, &header)
        .unwrap()
        .append_state_diff(header.block_header_without_hash.block_number, diff)
        .unwrap()
        .append_body(header.block_header_without_hash.block_number, Default::default())
        .unwrap()
        .commit()
        .unwrap();

    // The storage write path (write_deployed_contracts) will have inserted Nonce::default()
    // because it detects no prior nonce. So this test will actually pass with Ok(Nonce::default()).
    // That confirms the write path compensates for the read-path bug.
    //
    // The bug would manifest if you queried a contract_address that:
    //   1. has a class_hash (passes verify_contract_deployed), AND
    //   2. has no entry in the nonces table.
    // In that scenario, state_reader.get_nonce_at returns None and the caller
    // gets ContractNotFound instead of Nonce::default().
    let response = state_sync
        .handle_request(StateSyncRequest::GetNonceAt(
            header.block_header_without_hash.block_number,
            address,
        ))
        .await;
    let StateSyncResponse::GetNonceAt(result) = response else {
        panic!("Unexpected response");
    };
    // Storage wrote Nonce::default() automatically, so this succeeds:
    assert_eq!(result, Ok(Nonce::default()));

    // The code smell: if storage had NOT auto-inserted the nonce, the result would be:
    // Err(StateSyncError::ContractNotFound(address))
    // — the wrong error for a known-deployed contract.
}
```

**How to verify**:
```
SEED=0 cargo test -p apollo_state_sync test_get_nonce_at_wrong_error_when_nonce_missing
```

The test passes (because the storage layer saves us), but the written justification documents where the fix belongs: line 260 of `lib.rs` should use `.unwrap_or_default()` or map `None` to `Nonce::default()` rather than to `ContractNotFound`.

---

## Bug 3: `latest_synced_block` considers only state and body markers, not header marker — but `get_latest_block_header` depends on all three being in sync

**File**: `/home/user/sequencer/crates/apollo_state_sync/src/lib.rs`, lines 353–367 and 289–297

**Description**:
`latest_synced_block` returns `min(state_marker.prev(), body_marker.prev())`. It intentionally ignores the header marker because "sync always writes headers before other block data". The `get_latest_block_header` function uses this result as an index into `get_block_header`.

However, the header marker can be ahead of both the state and body markers. If `header_marker=5`, `state_marker=3`, `body_marker=3`, then `latest_synced_block` returns `Some(BlockNumber(2))`, and `get_block_header(2)` is called. This is fine — block 2's header exists.

But consider the inverse: is it possible for `header_marker < state_marker`? The comments say "we assume sync always writes headers first." If something violates this assumption (e.g., a storage bug or a non-standard write sequence in tests), `get_block_header(block_number)` at the end of `get_latest_block_header` would return `None`, making the function return `Ok(None)` while `get_latest_block_number` returns `Ok(Some(block_number))`. This inconsistency between the two APIs is observable:

```
get_latest_block_number() -> Some(N)
get_latest_block_header() -> None  // should be Some(header for block N)
```

This is a latent design bug: the function silently swallows the `None` instead of returning an error indicating a storage inconsistency.

**Root Cause**:
`get_latest_block_header` has no fallback to surface the case where `get_block_header` returns `None` for a block that `latest_synced_block` guarantees to be synced.

**Written Justification**:
This is hard to trigger in a well-tested runtime, but the fix is straightforward: change `get_latest_block_header` to return an error if `get_block_header` returns `None` after a successful `latest_synced_block` lookup:

```rust
// Current (silent None propagation — possible inconsistency):
match latest_block_number {
    Some(block_number) => Ok(txn.get_block_header(block_number)?),
    None => Ok(None),
}

// Corrected (explicitly surface storage inconsistency):
match latest_block_number {
    Some(block_number) => {
        txn.get_block_header(block_number)?
            .ok_or(StateSyncError::StorageError(format!(
                "Header missing for synced block {block_number}"
            )))
            .map(Some)
    }
    None => Ok(None),
}
```

**How to verify**:
Manually construct a `StorageWriter` that writes state and body for block 0 but skips writing the header, then call `get_latest_block_header`. Requires internal storage API access; no simpler public-API reproduction exists.

---

## Summary

| # | Severity | File | Description |
|---|----------|------|-------------|
| 1 | **High** | `lib.rs:299–337` | `is_cairo_1_class_declared_at` / `is_class_declared_at` silently return `false` instead of `BlockNotFound` for unsynced blocks |
| 2 | **Medium** | `lib.rs:258–262` | `get_nonce_at` maps a missing-nonce `None` to `ContractNotFound`, wrong error for a verified-deployed contract |
| 3 | **Low** | `lib.rs:289–297` | `get_latest_block_header` silently returns `None` if header is missing for a block that `latest_synced_block` claims is fully synced |
