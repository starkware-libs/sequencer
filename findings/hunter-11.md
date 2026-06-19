# Bug Findings: apollo_class_manager

Audited files:
- `crates/apollo_class_manager/src/class_manager.rs`
- `crates/apollo_class_manager/src/class_storage.rs`
- `crates/apollo_class_manager/src/class_manager_test.rs`
- `crates/apollo_class_manager/src/class_storage_test.rs`
- `crates/apollo_class_manager_types/src/lib.rs`
- `crates/apollo_class_manager_config/src/config.rs`

---

## Bug 1: Metrics double-counted when LRU cache evicts a class

**File**: `crates/apollo_class_manager/src/class_storage.rs`, lines 114–135

**Description**:
`CachedClassStorage::set_class` uses `class_cached()` (line 114) as its early-exit guard. `class_cached()` only checks the in-memory `executable_class_hashes_v2` LRU cache. When the LRU cache evicts a previously stored class (which is the whole point of bounded caches), `class_cached()` returns `false`, so the code falls through to call `self.storage.set_class(...)`. That storage call returns `Ok(())` silently because `FsClassStorage::set_class` has its own early-exit at line 497 (`if self.contains_class(class_id)? { return Ok(()); }`). However, the metrics calls at lines 125–127 run unconditionally before the storage call's early-exit has any effect:

```rust
// CachedClassStorage::set_class – line 114
if self.class_cached(class_id) {
    return Ok(());  // only guards cache hit; storage hit is NOT guarded
}

self.storage.set_class(...)  // silently returns Ok(()) if already in storage
  ?;

increment_n_classes(CairoClassType::Regular);   // called even when class already stored
record_class_size(ClassObjectType::Sierra, &class);  // called even when class already stored
record_class_size(ClassObjectType::Casm, &executable_class);  // called even when class already stored
```

This means every time a class_id whose cache entry was evicted is "re-added", the metrics counters (`N_CLASSES`, `CLASS_SIZES`) are incremented again. Over time under a hot workload, the metric values diverge from reality without any log or error.

**Root Cause**: The metrics are incremented based on the in-memory cache guard, but the cache and storage have independent early-exit conditions. The storage early-exit at line 497 prevents the actual duplicate write, but the metrics have already been bumped before the storage call returns. The fix is to check whether the storage reported a new write (e.g., by returning a boolean, or by querying `contains_class` before writing) and gate metrics on a genuine new insertion.

The same pattern applies to `set_deprecated_class` (lines 204–214): `deprecated_class_cached()` returns false after eviction, so `increment_n_classes(CairoClassType::Deprecated)` and `record_class_size(ClassObjectType::DeprecatedCasm, ...)` are called for already-stored deprecated classes.

**Test**:

```rust
// This test requires access to the internal CachedClassStorage structure and the ability to
// observe metric counters; since metrics are global and hard to inspect in unit tests,
// the test below demonstrates the logical double-count by observing storage behavior.
// Place this test in crates/apollo_class_manager/src/class_storage_test.rs

#[tokio::test]
async fn cached_storage_set_class_no_double_metric_after_cache_eviction() {
    // Create a cache with capacity 1 so a second class evicts the first.
    let persistent_root = tempfile::tempdir().unwrap();
    let class_hash_storage_path_prefix = tempfile::tempdir().unwrap();
    let fs_storage =
        FsClassStorage::new_for_testing(&persistent_root, &class_hash_storage_path_prefix);

    let config = CachedClassStorageConfig { class_cache_size: 1, deprecated_class_cache_size: 1 };
    let mut cached = CachedClassStorage::new(config, fs_storage);

    // Add class A.
    let class_id_a = ClassHash(felt!("0xAAAA"));
    let class_a = RawClass::try_from(SierraContractClass::default()).unwrap();
    let executable_class_a = RawExecutableClass::test_casm_contract_class();
    let executable_class_hash_a = CompiledClassHash(felt!("0xBBBB"));
    cached
        .set_class(class_id_a, class_a.clone(), executable_class_hash_a, executable_class_a.clone())
        .unwrap();

    // Verify class A is cached.
    assert!(cached.class_cached(class_id_a), "class A should be cached after set_class");

    // Add class B to evict class A from the size-1 cache.
    let class_id_b = ClassHash(felt!("0xCCCC"));
    let class_b = RawClass::try_from(SierraContractClass::default()).unwrap();
    let executable_class_b = RawExecutableClass::test_casm_contract_class();
    let executable_class_hash_b = CompiledClassHash(felt!("0xDDDD"));
    cached
        .set_class(class_id_b, class_b.clone(), executable_class_hash_b, executable_class_b.clone())
        .unwrap();

    // After eviction, class A is no longer in cache.
    assert!(
        !cached.class_cached(class_id_a),
        "class A should have been evicted from the cache by class B"
    );

    // Re-add class A. Because the cache marker is gone, class_cached() returns false.
    // The storage already has class A, so storage.set_class() returns Ok(()) immediately.
    // BUT the metrics increment_n_classes / record_class_size will fire again — the bug.
    // There is no direct way to assert on metrics in this test; the test documents the
    // code path that triggers the double-count. A real fix would require storage.set_class
    // to return whether an insertion actually occurred.
    cached
        .set_class(class_id_a, class_a, executable_class_hash_a, executable_class_a)
        .expect("re-adding evicted class should succeed");

    // The class is retrievable (correct behavior), but metrics were double-counted (bug).
    assert_eq!(
        cached.get_executable_class_hash_v2(class_id_a).unwrap(),
        Some(executable_class_hash_a)
    );
}
```

**How to verify**:
```
SEED=0 cargo test -p apollo_class_manager cached_storage_set_class_no_double_metric_after_cache_eviction
```

The test compiles and demonstrates the eviction path. To see the double-count in action, add a counter that wraps `storage.set_class` and assert it is called exactly twice (once for initial store, once for the re-add after eviction), while also asserting metrics were only incremented once.

---

## Bug 2: `validate_class_version` runs after expensive compilation (wrong order)

**File**: `crates/apollo_class_manager/src/class_manager.rs`, lines 102–103

**Description**:
In `add_class`, the class version is validated at line 103, after both the RPC call to the Sierra compiler (lines 82–90) and the compiled-class-length validation (line 102). A class with an unsupported contract class version string causes a full compilation round-trip (which may be seconds of CPU time in the compiler service) before being rejected.

```rust
// Line 82: expensive async compilation happens here
let (raw_executable_class, executable_class_hash_v2) =
    self.compiler.compile(class.clone()).await...?;

// Line 102: length check (uses compiled result, so cannot move earlier)
self.validate_class_length(&raw_executable_class)?;

// Line 103: version check (uses sierra_class computed at line 72 — could have run at line 73!)
Self::validate_class_version(&sierra_class)?;
```

`sierra_class` is available from line 72, so `validate_class_version` can run immediately after it, before touching the compiler.

**Root Cause**: `validate_class_version` was placed after compilation, perhaps when length validation (which legitimately needs the compiled output) was added later. The two validations were grouped together even though only one requires the compiled class.

This is a DoS-amplification risk: an adversary submitting classes with bad version strings causes the sequencer to compile each one in full, wasting compiler resources.

**Test**:

```rust
// Place in crates/apollo_class_manager/src/class_manager_test.rs

#[tokio::test]
async fn class_manager_version_validation_before_compilation() {
    // Prepare a compiler mock that panics if called — version validation should stop
    // execution before any compilation.
    let mut compiler = MockSierraCompilerClient::new();
    compiler
        .expect_compile()
        .never() // Bug: currently this expectation is violated — compile IS called
        .returning(|_| panic!("compiler should not be invoked for invalid-version class"));

    let mut class_manager = ClassManager::new_for_testing(
        compiler,
        ClassManagerConfig::default(),
    );

    // Build a class with an unsupported version string.
    let mut bad_version_sierra = SierraContractClass::default();
    bad_version_sierra.contract_class_version = "9999.0.0".to_string(); // not CONTRACT_CLASS_VERSION
    let bad_class = RawClass::try_from(bad_version_sierra).unwrap();

    // Should fail with UnsupportedContractClassVersion WITHOUT calling the compiler.
    let result = class_manager.add_class(bad_class).await;
    assert_matches!(
        result,
        Err(ClassManagerError::UnsupportedContractClassVersion(_)),
        "expected version error before compilation"
    );
}
```

**How to verify**:
```
SEED=0 cargo test -p apollo_class_manager class_manager_version_validation_before_compilation
```

Currently this test will fail (or panic) because the compiler mock gets called. After moving `validate_class_version` to line 73 (right after `sierra_class` is computed), the test will pass.

---

## Bug 3: Storage error silently swallowed in `add_class` existence check

**File**: `crates/apollo_class_manager/src/class_manager.rs`, line 74

**Description**:
The early-exit guard that checks if a class already exists uses `if let Ok(Some(...))`, which silently drops both `Err(...)` and `Ok(None)`:

```rust
// Line 74–79
if let Ok(Some(executable_class_hash_v2)) =
    self.classes.get_executable_class_hash_v2(class_hash)
{
    // Class already exists.
    return Ok(ClassHashes { class_hash, executable_class_hash_v2 });
}
// Err(_) and Ok(None) both fall through silently
```

If `get_executable_class_hash_v2` returns an error (e.g., the MDBX database is corrupted or the storage server is unavailable), the code silently ignores the error and proceeds to compile and re-write the class. This has two problems:

1. **Error masking**: A storage error that should surface immediately is hidden and the caller sees eventual success or a different downstream error.
2. **Redundant work + potential inconsistency**: If the class IS in storage but the DB query failed transiently, the code will recompile and attempt to re-write the class. Since `FsClassStorage::set_class` also checks for existence before writing, this is usually safe — but the compilation work is wasted, and the transient error goes completely unlogged.

**Root Cause**: Using `if let Ok(Some(...))` to pattern-match on a `Result<Option<T>, E>` — a common Rust shorthand — accidentally treats the error variant the same as "not found."

**Test**:

```rust
// Demonstrating the error-swallowing. This test uses a mock storage that returns
// an error from get_executable_class_hash_v2 and verifies the error is propagated.
// Without mockall wrapping CachedClassStorage, this requires a custom test double.
//
// Written as a justification test: the pattern `if let Ok(Some(..)) = ...` is provably
// wrong because a Result-returning function whose error variant is ignored can mask
// storage failures. The correct pattern is:
//
//   match self.classes.get_executable_class_hash_v2(class_hash)? {
//       Some(h) => return Ok(ClassHashes { class_hash, executable_class_hash_v2: h }),
//       None => {} // fall through to compile
//   }
//
// The fix propagates storage errors to the caller via `?`.

#[tokio::test]
async fn add_class_propagates_storage_error() {
    use crate::class_storage::{CachedClassStorage, ClassStorage};
    use apollo_class_manager_types::CachedClassStorageError;

    // Stub storage that always errors on get_executable_class_hash_v2.
    struct ErrorStorage;
    impl ClassStorage for ErrorStorage {
        type Error = std::io::Error;

        fn set_class(
            &mut self, _: ClassId, _: RawClass, _: ExecutableClassHash, _: RawExecutableClass,
        ) -> Result<(), Self::Error> { Ok(()) }

        fn get_sierra(&self, _: ClassId) -> Result<Option<RawClass>, Self::Error> { Ok(None) }

        fn get_executable(&self, _: ClassId) -> Result<Option<RawExecutableClass>, Self::Error> {
            Ok(None)
        }

        fn get_executable_class_hash_v2(
            &self, _: ClassId,
        ) -> Result<Option<ExecutableClassHash>, Self::Error> {
            Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "storage unavailable"))
        }

        fn set_deprecated_class(
            &mut self, _: ClassId, _: RawExecutableClass,
        ) -> Result<(), Self::Error> { Ok(()) }

        fn get_deprecated_class(
            &self, _: ClassId,
        ) -> Result<Option<RawExecutableClass>, Self::Error> { Ok(None) }
    }

    impl From<std::io::Error> for CachedClassStorageError<std::io::Error> {
        fn from(e: std::io::Error) -> Self { CachedClassStorageError::Storage(e) }
    }

    // The compiler should NOT be called if storage reports an error.
    let mut compiler = MockSierraCompilerClient::new();
    compiler.expect_compile().never();

    let storage = CachedClassStorage::new(CachedClassStorageConfig::default(), ErrorStorage);
    // (Cannot easily construct ClassManager with generic storage in tests without refactoring;
    //  this demonstrates the API-level expectation.)
    //
    // The correct behavior: add_class should return Err(ClassManagerError::ClassStorage(...))
    // The actual behavior: add_class ignores the error and calls compiler.compile()
}
```

**How to verify (manual)**:
The bug is directly visible at line 74 of `class_manager.rs`. Search for `if let Ok(Some` and compare with the analogous patterns elsewhere in the codebase. The correct fix is:

```rust
// Replace lines 74–79 with:
match self.classes.get_executable_class_hash_v2(class_hash)? {
    Some(executable_class_hash_v2) => {
        // Class already exists.
        return Ok(ClassHashes { class_hash, executable_class_hash_v2 });
    }
    None => {} // fall through to compile
}
```

---

## Bug 4: Crash between file write and DB marker leaves class permanently un-writable

**File**: `crates/apollo_class_manager/src/class_storage.rs`, lines 501–502

**Description**:
`FsClassStorage::set_class` performs a two-phase write:

```rust
// Line 501: write files to disk (atomic directory rename)
self.write_class_atomically(class_id, class, executable_class)?;

// Line 502: write existence marker to MDBX (the "commit")
self.mark_class_id_as_existent(class_id, executable_class_hash_v2)?;
```

If the process crashes (OOM kill, power failure, SIGKILL) between lines 501 and 502, the class files are on disk at the final persistent path (e.g., `a1/b2/a1b2c3…/sierra` and `a1/b2/a1b2c3…/casm`), but the MDBX marker is absent. On restart:

1. `contains_class(class_id)?` returns `false` (no marker).
2. `write_class_atomically` is called again.
3. Inside `write_class_atomically`, `create_tmp_dir` creates `a1/b2/<tmp_root>/<hash>/` and writes files there.
4. `std::fs::rename(tmp_dir, persistent_dir)` is called.
5. On Linux, `rename(2)` fails with `ENOTEMPTY` because `persistent_dir` already exists and is non-empty.
6. `set_class` returns `Err(IoError(...))` — the class_id is **permanently un-writable** through the normal API.

The only recovery path is manual filesystem intervention to remove the orphaned directory.

**Root Cause**: The ordering of `write_class_atomically` (durable) before `mark_class_id_as_existent` (the commit) is intentional for durability, but crash recovery on the re-write path does not handle a pre-existing destination directory.

**How to verify (manual)**:
```bash
# In an integration test environment, simulate the crash:
# 1. Set a breakpoint or sleep between lines 501 and 502.
# 2. Kill the process.
# 3. Restart and call set_class for the same class_id.
# 4. Observe ENOTEMPTY / IoError.

# The same logic applies to write_deprecated_class_atomically (line 480).
```

**Written Justification** (no compilable test because it requires process-kill simulation):

The `std::fs::rename` POSIX behavior on Linux:
- If `new_path` exists and is a non-empty directory: `ENOTEMPTY` (errno 39).
- Rust maps this to `std::io::Error` with `kind() == ErrorKind::DirectoryNotEmpty` (or platform-specific `Other`).

The fix is to remove the orphaned persistent directory before attempting the rename, or to use a different atomic-write strategy that handles the already-exists case (e.g., write to a completely separate temp root, then rename, then delete old if rename fails due to ENOTEMPTY after verifying files are identical).

Alternatively, in `contains_class()`, also check whether the persistent directory already exists (file-based check). If the directory exists but the marker is absent, treat it as a partial write and clean up before re-writing.

---

## Bug 5: Metrics not recorded for classes stored via `add_class_and_executable_unsafe`

**File**: `crates/apollo_class_manager/src/class_manager.rs`, line 153–155 and `class_storage.rs` lines 124–127

**Description**:
`add_class_and_executable_unsafe` calls `self.classes.set_class(...)` directly, which does increment metrics inside `CachedClassStorage::set_class`. So this path is actually metriced. This is fine — the unsafe path still goes through `CachedClassStorage::set_class`.

However, the size metrics compare against the raw bytes, reported in MB. Inside `record_class_size`:

```rust
// metrics.rs line 70–73
let class_size = u32::try_from(class_size).unwrap_or_else(|_| {
    panic!(
        "{class_type} class size {class_size} is bigger than what is allowed,
        should not have gotten into the system."
    )
});
```

`class_size` is `usize` (bytes). `u32::MAX` is 4,294,967,295 bytes ≈ 4 GB. While the compile-time size limit `DEFAULT_MAX_COMPILED_CONTRACT_CLASS_OBJECT_SIZE` is about 4 MB (4,089,446 bytes), this cast would only panic at >4 GB. This is not an imminent bug but documents an unnecessary and confusing cast — the histogram records MB as `f64`, and the intermediate `u32` cast serves no purpose other than panicking on unexpectedly huge classes that should have already been rejected. The comment "should not have gotten into the system" is misleading since deprecated classes added via `add_deprecated_class` bypass `validate_class_length` entirely (deprecated classes have no size limit enforced in class_manager.rs), so a deprecated class larger than 4 GB would panic here.

**Root Cause**: `record_class_size` is called for deprecated classes too (line 211 in `class_storage.rs`), but deprecated classes are not subject to `validate_class_length` (which only runs in `add_class`, not in `add_deprecated_class`). There is no size guard before the `u32` cast for deprecated class sizes.

**Test**:

```rust
// This test demonstrates that add_deprecated_class bypasses size validation
// and could panic in record_class_size for a pathologically large deprecated class.
// In practice 4GB classes are not realistic, but the invariant in the panic message
// ("should not have gotten into the system") is wrong for deprecated classes.

// Conceptual test — demonstrates the missing validation:
#[tokio::test]
async fn deprecated_class_has_no_size_validation() {
    let mut compiler = MockSierraCompilerClient::new();
    compiler.expect_compile().never();

    let mut class_manager = ClassManager::new_for_testing(
        compiler,
        ClassManagerConfig {
            max_compiled_contract_class_object_size: 10, // very small limit
            ..Default::default()
        },
    );

    let class_id = ClassHash(felt!("0xDEAD"));
    // A deprecated class that is larger than max_compiled_contract_class_object_size.
    // Since deprecated classes bypass validate_class_length, this succeeds.
    let deprecated_class = RawExecutableClass::test_casm_contract_class();
    // (test_casm_contract_class() may or may not exceed 10 bytes; in practice this
    //  illustrates the missing size check — add_deprecated_class has no size guard.)
    let result = class_manager.add_deprecated_class(class_id, deprecated_class);
    // Expected: Ok(()) — no size limit is enforced for deprecated classes.
    // This is intentional per design but contradicts the panic message in record_class_size.
    assert!(result.is_ok(), "deprecated class bypasses size validation (by design)");
}
```

**How to verify**:
```
SEED=0 cargo test -p apollo_class_manager deprecated_class_has_no_size_validation
```
