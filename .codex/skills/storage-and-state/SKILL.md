---
name: storage-and-state
description: Use this skill for work in `apollo_storage`, `apollo_committer`, `starknet_patricia`, `starknet_patricia_storage`, or any task involving MDBX, RocksDB, global roots, state diffs, storage readers/writers, or storage-backed tests. It should also trigger for on-disk compatibility questions.
---

# Storage and State

<purpose>
Change storage-backed code without breaking lock semantics, reader/writer contracts, or commitment flows.
</purpose>

<context>
- `apollo_storage` is the main libmdbx-backed storage layer for sequencer state.
- `apollo_committer` uses `starknet_patricia_storage::rocksdb_storage::RocksDbStorage` for Patricia/global-root computation.
- Tests already encode important storage behavior: single-writer MDBX locking, `TempDir` usage, and storage reader server flows.
</context>

<procedure>
1. Identify the storage domain:
   - sequencer state / headers / diffs -> `apollo_storage`
   - Patricia tree / global root -> `starknet_patricia*` and `apollo_committer`
2. Keep reader and writer traits in sync when adding or changing storage behavior.
3. Treat on-disk contract changes as gated. Ordinary bug fixes inside current storage semantics are fine.
4. For tests:
   - create a fresh `TempDir`
   - keep the directory alive for the full test scope
   - avoid shared database paths across processes
5. If storage changes affect consensus, batcher, or state sync behavior, run cross-crate verification after local tests.
</procedure>

<patterns>
<do>
- Use existing storage traits and transaction wrappers instead of inventing side paths.
- Follow the established `StorageReader` / `StorageWriter` split.
- Let tests prove lock and path behavior rather than relying on assumptions.
</do>
<dont>
- Don't hardcode storage paths in tests.
- Don't assume MDBX supports multiple writers.
- Don't change storage layout or persisted meaning without approval and explicit compatibility review.
</dont>
</patterns>

<examples>
Example: storage-test setup
```rust
let temp_dir = tempfile::TempDir::new().unwrap();
let ((reader, writer), _config, _temp_dir) =
    apollo_storage::test_utils::get_test_storage_by_scope(StorageScope::StateOnly);
```
</examples>

<troubleshooting>
| Symptom | Cause | Fix |
|---------|-------|-----|
| Exclusive lock / open failure | MDBX path reused across writers or processes | allocate a unique temp directory |
| Global root mismatch | committer and Patricia state diverged | trace `apollo_committer` RocksDB path and root-loading flow |
| Storage tests flake on path lifetime | temp dir dropped too early | keep `TempDir` in scope until the test ends |
</troubleshooting>

<references>
- `crates/apollo_storage/src/db/mod.rs`: low-level MDBX environment and transaction layer
- `crates/apollo_storage/src/test_utils.rs`: canonical test-storage helpers
- `crates/apollo_storage/tests/open_storage_in_processes_test.rs`: lock semantics
- `crates/apollo_committer/src/committer.rs`: RocksDB-backed commitment flow
- `crates/starknet_patricia_storage/src/rocksdb_storage.rs`: Patricia RocksDB implementation
</references>
