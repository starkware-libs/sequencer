# Bug Hunter 6 Findings

## Files Examined

- `crates/apollo_central_sync/src/lib.rs` — main sync orchestration, block/state-diff/compiled-class streams, `check_sync_progress`
- `crates/apollo_central_sync/src/pending_sync.rs` — pending block sync loop
- `crates/apollo_central_sync/src/sources/central.rs` — central source trait and implementation
- `crates/apollo_central_sync/src/sources/central/state_update_stream.rs` — streaming state updates with class downloads
- `crates/apollo_central_sync/src/sources/base_layer.rs` — base layer source
- `crates/apollo_central_sync/src/sync_test.rs` — integration tests for state sync
- `crates/apollo_central_sync/src/sources/central_sync_test.rs` — unit tests for central sync
- `crates/apollo_central_sync/src/sources/central_test.rs` — unit tests for central source
- `crates/apollo_state_sync/src/runner/mod.rs` — runner wiring
- `crates/papyrus_common/src/pending_classes.rs` — PendingClasses storage structure

---

## Bug 1

**File**: `crates/apollo_central_sync/src/lib.rs`
**Location**: `fn check_sync_progress`, line ~1029
**Description**: The progress-check condition uses `||` (OR) instead of `&&` (AND), causing spurious `NoProgress` errors during normal operation.

**Root Cause**:
```rust
if header_marker==new_header_marker || state_marker==new_state_marker || is_casm_stuck {
    yield SyncEvent::NoProgress;
}
```
This fires `NoProgress` if **any** one of the three markers is unchanged, even if the other two are advancing. Normal sync operation routinely has one stream fully caught up while the others are still progressing — for example, headers are synced to the tip while state diffs catch up, causing `header_marker == new_header_marker` to be true every interval.

The correct logic is `&&` (AND): only declare no progress if **all** relevant streams are simultaneously stuck. The code even carries the comment `// TODO(DvirYo): fix the bug and remove this function.` at line 998, confirming this is a known defect.

The consequence is periodic spurious sync restarts via the recoverable `StateSyncError::NoProgress` path in `GenericStateSync::run`. The `SLEEP_TIME_SYNC_PROGRESS` constant is 300 seconds, so in production this fires every 5 minutes whenever headers are ahead of state updates.

**Failing Test**:

```rust
// In crates/apollo_central_sync/src/sync_test.rs

#[tokio::test]
async fn check_sync_progress_no_false_positive_when_headers_fully_synced_but_state_catching_up() {
    use apollo_storage::header::HeaderStorageWriter;
    use apollo_storage::state::StateStorageWriter;
    use apollo_storage::test_utils::get_test_storage;
    use futures_util::StreamExt;
    use starknet_api::block::{BlockHeader, BlockHeaderWithoutHash, BlockNumber};
    use starknet_api::state::ThinStateDiff;
    use indexmap::indexmap;
    use crate::check_sync_progress;

    let ((reader, mut writer), _temp_dir) = get_test_storage();

    // Simulate: headers fully synced to block 5, state diffs only at block 3.
    // This is a normal healthy state: header stream is ahead, state diff stream is catching up.
    for i in 0..5u64 {
        writer
            .begin_rw_txn()
            .unwrap()
            .append_header(
                BlockNumber(i),
                &BlockHeader {
                    block_header_without_hash: BlockHeaderWithoutHash {
                        block_number: BlockNumber(i),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )
            .unwrap()
            .commit()
            .unwrap();
    }
    for i in 0..3u64 {
        writer
            .begin_rw_txn()
            .unwrap()
            .append_state_diff(BlockNumber(i), ThinStateDiff::default())
            .unwrap()
            .commit()
            .unwrap();
    }

    // header_marker = 5, state_marker = 3, casm_marker = 0
    // The sync is healthy: state diffs and casm are still progressing (we simulate one
    // more state diff being written below), but headers are fully caught up.
    // check_sync_progress should NOT fire NoProgress in this scenario.

    // Capture the initial state that check_sync_progress will record.
    // The function sleeps SLEEP_TIME_SYNC_PROGRESS (300s) before checking, so we advance
    // storage *before* the stream can read the new state. We drive the stream via
    // tokio::time::pause/advance to avoid real sleeps.
    tokio::time::pause();

    let store_sierras_and_casms_block_threshold = u64::MAX; // threshold disabled
    let mut progress_stream =
        check_sync_progress(reader.clone(), store_sierras_and_casms_block_threshold).boxed();

    // Advance time by SLEEP_TIME_SYNC_PROGRESS (300s) so the stream wakes up.
    // Meanwhile, write one more state diff to simulate progress in the state stream.
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(3), ThinStateDiff::default())
        .unwrap()
        .commit()
        .unwrap();

    tokio::time::advance(std::time::Duration::from_secs(301)).await;

    // The check should NOT yield NoProgress because state_marker advanced (3 → 4),
    // even though header_marker was stuck at 5 the whole time.
    // BUG: with || the condition `header_marker == new_header_marker` (5 == 5) is true,
    // so NoProgress IS incorrectly yielded.
    let event = tokio::time::timeout(
        std::time::Duration::from_millis(100),
        progress_stream.next(),
    )
    .await;

    // If the bug is present, the stream yields Some(Ok(SyncEvent::NoProgress)).
    // The correct behavior is that no event is yielded (timeout), or if one is yielded
    // it should NOT be NoProgress.
    match event {
        // Timeout means no spurious NoProgress was emitted — correct behavior.
        Err(_timeout) => {}
        Ok(Some(Ok(event))) => {
            // With the bug, this assertion fails because event IS NoProgress.
            assert!(
                !matches!(event, crate::SyncEvent::NoProgress),
                "check_sync_progress fired NoProgress even though state_marker advanced \
                 (headers were merely fully caught up, not stuck)"
            );
        }
        Ok(other) => panic!("Unexpected stream result: {:?}", other),
    }
}
```

**How to Verify**: The bug is acknowledged in the source at line 998:
```rust
// TODO(DvirYo): fix the bug and remove this function.
```
The condition on line 1029 should be `&&` instead of `||`. Changing it to `&&` makes the function only fire `NoProgress` when all three markers simultaneously fail to advance.

---

## Bug 2

**File**: `crates/apollo_central_sync/src/pending_sync.rs`
**Location**: `fn sync_pending_data`, lines ~62–97
**Description**: Compiled class download deduplication uses `compiled_class_hash` as the key, but the download is keyed by `class_hash`. When two different Sierra classes in the same pending block share the same `compiled_class_hash`, the second class's CASM is never downloaded and never added to `PendingClasses`.

**Root Cause**:
```rust
let mut processed_compiled_classes = HashSet::new();  // tracks CompiledClassHash
// ...
for DeclaredClassHashEntry { class_hash, compiled_class_hash } in declared_classes {
    // ...
    if processed_compiled_classes.insert(compiled_class_hash) {  // deduplicate by compiled_class_hash
        tasks.push(
            get_pending_compiled_class(
                class_hash,  // but download/store by class_hash
                central_source.clone(),
                pending_classes.clone(),
            )
            .boxed(),
        );
    }
}
```

`PendingClasses::add_compiled_class` and `get_compiled_class` are keyed by `ClassHash` (Sierra class hash), not `CompiledClassHash`. So if class A (`class_hash=A, compiled_class_hash=X`) is processed first, and then class B (`class_hash=B, compiled_class_hash=X`) arrives in the same or a later pending update within the same `sync_pending_data` call, the guard `processed_compiled_classes.insert(X)` returns `false` and the download for class B is skipped. As a result, `pending_classes.compiled_classes` has an entry for A but not for B, and any RPC call asking for the CASM of class B in the pending block will get `None`.

The `processed_compiled_classes` set should be keyed by `ClassHash` (same as `processed_classes`), not `CompiledClassHash`, since the download and storage are both keyed by `class_hash`.

**Failing Test**:

```rust
// In crates/apollo_central_sync/src/sync_test.rs
// This test can be added alongside the existing pending sync tests.

#[tokio::test]
async fn pending_sync_downloads_compiled_class_for_each_class_hash_even_when_compiled_class_hash_is_shared() {
    use apollo_starknet_client::reader::objects::pending_data::{
        DeprecatedPendingBlock, PendingBlockOrDeprecated, PendingStateUpdate,
    };
    use apollo_starknet_client::reader::{DeclaredClassHashEntry, PendingData};
    use apollo_storage::test_utils::get_test_storage;
    use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
    use papyrus_common::pending_classes::{ApiContractClass, PendingClasses, PendingClassesTrait};
    use starknet_api::block::BlockHash;
    use starknet_api::core::{ClassHash, CompiledClassHash};
    use starknet_api::hash::StarkHash;
    use starknet_api::state::SierraContractClass;
    use apollo_test_utils::{get_rng, GetTestInstance};
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::RwLock;

    use crate::sources::central::MockCentralSourceTrait;
    use crate::sources::pending::MockPendingSourceTrait;
    use crate::sync_pending_data;

    let genesis_parent_hash = BlockHash::GENESIS_PARENT_HASH;
    let (reader, _) = get_test_storage().0;
    let mut rng = get_rng();

    // Two distinct Sierra class hashes that share the same compiled class hash.
    let class_hash_a = ClassHash(StarkHash::ONE);
    let class_hash_b = ClassHash(StarkHash::TWO);
    let shared_compiled_class_hash = CompiledClassHash(StarkHash::THREE);

    // One pending update declaring both classes with the same compiled_class_hash.
    let pending_data_with_two_classes = PendingData {
        block: PendingBlockOrDeprecated::Deprecated(DeprecatedPendingBlock {
            parent_block_hash: genesis_parent_hash,
            ..Default::default()
        }),
        state_update: PendingStateUpdate {
            state_diff: apollo_starknet_client::reader::objects::state::StateDiff {
                declared_classes: vec![
                    DeclaredClassHashEntry {
                        class_hash: class_hash_a,
                        compiled_class_hash: shared_compiled_class_hash,
                    },
                    DeclaredClassHashEntry {
                        class_hash: class_hash_b,
                        compiled_class_hash: shared_compiled_class_hash, // same compiled hash!
                    },
                ],
                ..Default::default()
            },
            ..Default::default()
        },
    };

    // A second pending update that signals a new block is being built (causing loop exit).
    let new_block_pending_data = PendingData {
        block: PendingBlockOrDeprecated::Deprecated(DeprecatedPendingBlock {
            parent_block_hash: BlockHash(StarkHash::ONE),
            ..Default::default()
        }),
        ..Default::default()
    };

    let casm_for_a = CasmContractClass::get_test_instance(&mut rng);
    let casm_for_b = CasmContractClass::get_test_instance(&mut rng);
    // Make them distinct so we can verify the right one was stored.
    let casm_for_a_clone = casm_for_a.clone();
    let casm_for_b_clone = casm_for_b.clone();

    let mut mock_pending_source = MockPendingSourceTrait::new();
    mock_pending_source
        .expect_get_pending_data()
        .times(1)
        .return_once(move || Ok(pending_data_with_two_classes));
    mock_pending_source
        .expect_get_pending_data()
        .times(1)
        .return_once(move || Ok(new_block_pending_data));

    let mut mock_central_source = MockCentralSourceTrait::new();
    // Both class A and class B need their Sierra class downloaded.
    mock_central_source
        .expect_get_class()
        .withf(move |&ch| ch == class_hash_a)
        .times(1)
        .return_once(|_| Ok(ApiContractClass::ContractClass(SierraContractClass::default())));
    mock_central_source
        .expect_get_class()
        .withf(move |&ch| ch == class_hash_b)
        .times(1)
        .return_once(|_| Ok(ApiContractClass::ContractClass(SierraContractClass::default())));
    // Both class A and class B need their CASM downloaded, keyed by class_hash.
    mock_central_source
        .expect_get_compiled_class()
        .withf(move |&ch| ch == class_hash_a)
        .times(1)
        .return_once(move |_| Ok(casm_for_a_clone));
    // BUG: this expectation will NOT be satisfied because the second get_compiled_class
    // call for class_hash_b is skipped by the deduplication logic.
    mock_central_source
        .expect_get_compiled_class()
        .withf(move |&ch| ch == class_hash_b)
        .times(1)  // This will fail: mockall will see 0 calls
        .return_once(move |_| Ok(casm_for_b_clone));

    let pending_data_lock = Arc::new(RwLock::new(PendingData::default()));
    let pending_classes_lock = Arc::new(RwLock::new(PendingClasses::default()));

    sync_pending_data(
        reader,
        Arc::new(mock_central_source),
        Arc::new(mock_pending_source),
        pending_data_lock.clone(),
        pending_classes_lock.clone(),
        Duration::ZERO,
    )
    .await
    .unwrap();

    // After sync, both class A and class B should have their compiled classes stored.
    let classes = pending_classes_lock.read().await;

    // This assertion passes (class A was processed first).
    assert!(
        classes.get_compiled_class(class_hash_a).is_some(),
        "CASM for class_hash_a should be stored"
    );

    // BUG: This assertion fails because class B's CASM was never downloaded.
    // The deduplication guard on `compiled_class_hash` prevented the second download,
    // even though PendingClasses stores entries keyed by class_hash, not compiled_class_hash.
    assert!(
        classes.get_compiled_class(class_hash_b).is_some(),
        "CASM for class_hash_b should be stored even though it shares compiled_class_hash with class_hash_a"
    );
}
```

**How to Verify**: `cargo test -p apollo_central_sync pending_sync_downloads_compiled_class_for_each_class_hash_even_when_compiled_class_hash_is_shared`

The test will fail in two ways with the current code:
1. The `times(1)` expectation on `get_compiled_class` for `class_hash_b` will not be satisfied (mockall will panic or the check will fail at drop).
2. The final assertion `classes.get_compiled_class(class_hash_b).is_some()` will fail.

**Fix**: Change `processed_compiled_classes` to be a `HashSet<ClassHash>` (same type as `processed_classes`) and insert `class_hash` instead of `compiled_class_hash`:

```rust
// Before:
let mut processed_compiled_classes = HashSet::new();
// ...
if processed_compiled_classes.insert(compiled_class_hash) {
    tasks.push(get_pending_compiled_class(class_hash, ...).boxed());
}

// After:
// (remove processed_compiled_classes entirely, reuse processed_classes for both,
//  or use a separate set keyed by class_hash)
if processed_classes.contains(&class_hash) {
    // already scheduling class download; also schedule compiled class
    // Actually: the correct fix is to use class_hash as the dedup key
}
```

The simplest correct fix is to track compiled class downloads by `class_hash`, not `compiled_class_hash`, since that is both the download key and the storage key.
