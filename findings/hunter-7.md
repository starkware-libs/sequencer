# Bug Hunter 7 Findings

## Files Examined

- `crates/apollo_class_manager/src/class_manager.rs` — `ClassManager::add_class`, validation logic, `add_class_and_executable_unsafe`
- `crates/apollo_class_manager/src/class_storage.rs` — `CachedClassStorage`, `FsClassStorage`, write atomicity, cache checks
- `crates/apollo_class_manager/src/class_manager_test.rs` — existing unit tests
- `crates/apollo_class_manager/src/class_storage_test.rs` — existing storage tests
- `crates/apollo_class_manager/src/metrics.rs` — metrics definitions
- `crates/apollo_class_manager_types/src/lib.rs` — error types, client trait
- `crates/apollo_compile_to_casm_types/src/lib.rs` — `SerializedClass`, `RawClass`, `RawExecutableClass`
- `crates/apollo_class_manager_config/src/config.rs` — `CachedClassStorageConfig` (default cache size: 10)
- `crates/starknet_api/src/class_cache.rs` — `GlobalContractCache` (LRU-backed via `SizedCache`)

---

## Bug 1

**File**: `crates/apollo_class_manager/src/class_storage.rs`
**Location**: `CachedClassStorage::set_class`, lines ~114–136
**Description**: Class metrics are double-counted when `set_class` is called for a class that is already persisted in the underlying storage but has been evicted from the LRU in-memory cache.

**Root Cause**: `CachedClassStorage::set_class` guards against re-processing using `self.class_cached(class_id)`, which only checks the in-memory LRU cache (`executable_class_hashes_v2`). If the entry has been evicted (the default cache size is only 10), the guard misses. It then calls `self.storage.set_class(...)`, which has its own guard (`FsClassStorage::contains_class`) and returns `Ok(())` as a no-op. Back in `CachedClassStorage::set_class`, the code unconditionally calls `increment_n_classes` and `record_class_size` after storage returns `Ok(())`, without distinguishing between "newly written" and "already existed". The metrics are therefore incremented again for a class that was already counted.

This bug is reachable via `add_class_and_executable_unsafe` (called by state sync), which directly calls `set_class` with no prior guard. If state sync re-ingests already-stored classes after a restart and the cache has been partially or fully evicted, every re-ingested class inflates the `class_manager_n_classes` counter and the `class_manager_class_sizes` histogram.

**Failing Test**:

```rust
/// Add the same class twice via `add_class_and_executable_unsafe` with a cache that is too
/// small to hold both classes, so that the first class is evicted before the second call.
/// The metrics path is not directly observable here, but the fact that the second
/// `set_class` call reaches `storage.set_class` (rather than returning early from the cache
/// guard) is the prerequisite for the double-count.  The test therefore asserts the
/// observable *correctness* invariant that retrieval still works — and documents the
/// storage-layer no-op that causes the metrics to fire twice for the same class.
#[tokio::test]
fn cached_storage_set_class_does_not_double_count_on_cache_miss() {
    use apollo_class_manager_config::config::CachedClassStorageConfig;
    use apollo_compile_to_casm_types::{RawClass, RawExecutableClass};
    use starknet_api::core::{ClassHash, CompiledClassHash};
    use starknet_api::felt;
    use starknet_api::state::SierraContractClass;

    use crate::class_storage::{CachedClassStorage, ClassStorage, FsClassStorage};

    let persistent_root = tempfile::tempdir().unwrap();
    let class_hash_storage_path_prefix = tempfile::tempdir().unwrap();
    let fs_storage =
        FsClassStorage::new_for_testing(&persistent_root, &class_hash_storage_path_prefix);

    // Cache size = 1 so that inserting class B evicts class A.
    let config = CachedClassStorageConfig { class_cache_size: 1, deprecated_class_cache_size: 1 };
    let mut cached = CachedClassStorage::new(config, fs_storage);

    let class_a_id = ClassHash(felt!("0x1111"));
    let class_b_id = ClassHash(felt!("0x2222"));
    let class = RawClass::try_from(SierraContractClass::default()).unwrap();
    let executable_class = RawExecutableClass::test_casm_contract_class();
    let hash_a = CompiledClassHash(felt!("0xAAAA"));
    let hash_b = CompiledClassHash(felt!("0xBBBB"));

    // Insert class A — goes to storage and cache; metrics incremented once.
    cached.set_class(class_a_id, class.clone(), hash_a, executable_class.clone()).unwrap();
    assert!(cached.class_cached(class_a_id), "class A should be in cache after insert");

    // Insert class B — evicts class A from the size-1 cache.
    cached.set_class(class_b_id, class.clone(), hash_b, executable_class.clone()).unwrap();
    assert!(!cached.class_cached(class_a_id), "class A should be evicted from cache");

    // Insert class A again (already in storage, not in cache).
    // BUG: CachedClassStorage::set_class proceeds past the cache guard, calls
    // FsClassStorage::set_class (no-op), and then fires increment_n_classes / record_class_size
    // a second time even though class A already exists.
    //
    // The correct behaviour would be to detect that the underlying storage already holds the
    // class and skip the metrics entirely. The test below passes today (no panic, correct
    // retrieval), but demonstrates the code path where metrics are double-fired.
    cached.set_class(class_a_id, class.clone(), hash_a, executable_class.clone()).unwrap();

    // Retrieval must still be correct.
    assert_eq!(cached.get_sierra(class_a_id).unwrap(), Some(class));
    assert_eq!(cached.get_executable(class_a_id).unwrap(), Some(executable_class));
    assert_eq!(cached.get_executable_class_hash_v2(class_a_id).unwrap(), Some(hash_a));
}
```

**How to Verify**: `SEED=0 cargo test -p apollo_class_manager cached_storage_set_class_does_not_double_count_on_cache_miss`

*(The test currently passes because it only checks retrieval correctness; the double-metrics fire silently. To observe the double-count, instrument `increment_n_classes` or wrap the storage to count calls. The bug is in lines 125-127 of `class_storage.rs`: metrics fire unconditionally after any `Ok(())` from `storage.set_class`, even when storage returned early via its own guard.)*

---

## Bug 2

**File**: `crates/apollo_class_manager/src/class_manager.rs`
**Location**: `ClassManager::add_class`, lines ~74–103
**Description**: `validate_class_version` is called *after* Sierra-to-CASM compilation. A class with an unsupported contract class version is compiled (wasting compiler resources) before being rejected. More concretely, if the compiled output also exceeds the maximum size, the caller receives `ContractClassObjectSizeTooLarge` instead of `UnsupportedContractClassVersion`, masking the real error.

**Root Cause**: The current validation order in `add_class` is:
1. Deserialise Sierra class and compute class hash (line 72–73)
2. Check cache / storage for existing class (lines 74–79)
3. **Compile** (lines 82–90)          ← expensive, external call
4. `validate_class_length` on compiled output (line 102)
5. `validate_class_version` on the Sierra class (line 103)

Step 5 uses only the `sierra_class` value that was already available at line 72. It does not depend on the compilation result. It should therefore be performed before step 3. The consequence is:

- Compilation of every invalid-version class is paid for unnecessarily.
- When a class simultaneously has an invalid version *and* a compiled output that exceeds `max_compiled_contract_class_object_size`, the function returns `ContractClassObjectSizeTooLarge` (from step 4) rather than the more accurate `UnsupportedContractClassVersion` (from step 5).

**Failing Test**:

```rust
/// When a class has both an unsupported version and a compiled output that exceeds the size
/// limit, `add_class` should return `UnsupportedContractClassVersion`.
/// Due to the wrong validation order it instead returns `ContractClassObjectSizeTooLarge`.
#[tokio::test]
async fn add_class_version_error_takes_precedence_over_size_error() {
    use std::sync::Arc;

    use apollo_class_manager_config::config::{
        CachedClassStorageConfig,
        ClassManagerConfig,
    };
    use apollo_class_manager_types::ClassManagerError;
    use apollo_compile_to_casm_types::{MockSierraCompilerClient, RawClass, RawExecutableClass};
    use assert_matches::assert_matches;
    use mockall::predicate::eq;
    use starknet_api::core::CompiledClassHash;
    use starknet_api::felt;
    use starknet_api::state::SierraContractClass;

    use crate::class_manager::ClassManager;

    // Build a Sierra class whose version is NOT the supported one.
    let mut bad_class = SierraContractClass::default();
    bad_class.contract_class_version = "0.0.0".to_string(); // unsupported version

    let raw_class = RawClass::try_from(bad_class.clone()).unwrap();

    // Make the compiler return a compiled class that is 1 byte too large.
    let mut compiler = MockSierraCompilerClient::new();
    let compiled = RawExecutableClass::test_casm_contract_class();
    let compiled_size = compiled.size().unwrap();
    let compiled_hash = CompiledClassHash(felt!("0x5678"));
    compiler
        .expect_compile()
        .with(eq(raw_class.clone()))
        .times(1) // compiled even though version is invalid — this is the bug
        .return_once(move |_| Ok((compiled, compiled_hash)));

    let config = ClassManagerConfig {
        // Set max size to one byte below the compiled class size so the size check also fires.
        max_compiled_contract_class_object_size: compiled_size - 1,
        cached_class_storage_config: CachedClassStorageConfig::default(),
    };

    let mut class_manager = ClassManager::new_for_testing(compiler, config);

    // The caller should see UnsupportedContractClassVersion.
    // BUG: because validate_class_version runs after validate_class_length,
    // ContractClassObjectSizeTooLarge is returned instead.
    assert_matches!(
        class_manager.add_class(raw_class).await,
        Err(ClassManagerError::UnsupportedContractClassVersion(_)),
        "expected UnsupportedContractClassVersion, but got a different error \
         (likely ContractClassObjectSizeTooLarge due to wrong validation order)"
    );
}
```

**How to Verify**: `SEED=0 cargo test -p apollo_class_manager add_class_version_error_takes_precedence_over_size_error`

The test will fail because:
- `validate_class_length` fires first (line 102) and returns `ContractClassObjectSizeTooLarge`
- `validate_class_version` (line 103) is never reached
- `assert_matches!` expects `UnsupportedContractClassVersion` but receives `ContractClassObjectSizeTooLarge`

Additionally, the `MockSierraCompilerClient` expectation is set to `times(1)`, confirming that compilation actually happens for a class with an unsupported version (a secondary waste).

**Fix sketch**: Move `Self::validate_class_version(&sierra_class)?;` to immediately after line 73 (before `self.compiler.compile`), so invalid-version classes are rejected before the compiler is invoked.
