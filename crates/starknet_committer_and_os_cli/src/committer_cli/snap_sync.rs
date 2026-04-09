use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use apollo_committer_config::config::ApolloStorage;
use apollo_storage::state::StateStorageReader;
use apollo_storage::{StorageReader, StorageResult};
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, ContractAddress, PatriciaKey, MAX_PATRICIA_FELT};
use starknet_api::hash::StateRoots;
use starknet_api::state::{StateNumber, StorageKey};
use starknet_committer::block_committer::commit::{CommitBlockImpl, CommitBlockTrait};
use starknet_committer::block_committer::input::{
    Input,
    ReaderConfig,
    StarknetStorageKey,
    StarknetStorageValue,
    StateDiff,
};
use starknet_committer::block_committer::measurements_util::NoMeasurements;
use starknet_committer::db::forest_trait::{EmptyInitialReadContext, ForestWriterWithMetadata};
use starknet_committer::db::index_db::{IndexDb, IndexDbReadContext};
use starknet_committer::patricia_merkle_tree::types::CompiledClassHash as CommitterCompiledClassHash;
use starknet_patricia_storage::storage_trait::Storage;
use starknet_types_core::felt::{Felt, NonZeroFelt};
use tokio::sync::Mutex;
use tracing::info;

#[cfg(test)]
#[path = "snap_sync_test.rs"]
mod snap_sync_test;

/// Given the first leaf `start` and the felt of the last key seen (`last_key`), returns the
/// inclusive end of the largest valid Patricia subtree starting at `start` that doesn't exceed
/// `last_key`.
///
/// A valid Patricia subtree of size `2^k` requires `start % 2^k == 0`, so the size is capped
/// by the alignment of `start`.
fn compute_actual_end(start: Felt, last_key: Felt) -> Felt {
    let covered = last_key - start + Felt::ONE;
    // This is the largest number of bits, `x`, such that 2^x <= covered.
    // This is an upper bound for k.
    let max_contained_bits = u64::try_from(covered.bits()).expect("covered bits fits in u64") - 1;
    let exponent = if start == Felt::ZERO {
        max_contained_bits
    } else {
        // Equivalent to the largest `k` such that `2^k` divides `felt`.
        let trailing_zeros =
            start.to_biguint().trailing_zeros().expect("trailing_zeros called with zero");
        max_contained_bits.min(trailing_zeros)
    };
    start + Felt::TWO.pow(exponent) - Felt::ONE
}

/// Filters `entries` to those within the actual aligned Patricia subtree end, and returns that end.
///
/// - Fewer than `limit` entries: all entries are returned with the full requested `end`.
/// - Greater than or equal to `limit` entries: entries are filtered to `[start, actual_end]` where
///   `actual_end` is the inclusive end of the largest aligned Patricia subtree starting at `start`
///   that fits within the last key returned.
///
/// Panics if `limit` is 0.
fn shrink_to_actual_end<K: TreeKey, V>(
    mut entries: Vec<(K, V)>,
    start: K,
    end: K,
    limit: usize,
) -> (Vec<(K, V)>, Felt) {
    // TODO(yoav): return error if limit is 0.
    assert!(limit > 0, "limit must be positive");
    if entries.len() < limit {
        (entries, end.into())
    } else {
        let start_felt: Felt = start.into();
        let last_key: Felt = entries.last().expect("non-empty scan has a last entry").0.into();
        let actual_end = compute_actual_end(start_felt, last_key);
        entries
            .truncate(entries.partition_point(|(key, _)| Into::<Felt>::into(*key) <= actual_end));
        (entries, actual_end)
    }
}

/// Identifies which Patricia tree a request targets.
/// Trait for Patricia tree key types used in `TreeRequest`.
///
/// `Context` carries any per-request metadata needed by `scan`.
#[allow(dead_code)]
trait TreeKey: Copy + Default + Into<Felt> + Send + Sync + 'static {
    type Context: Clone + Send + Sync + 'static;

    /// Returns the inclusive maximum key value for this key type.
    fn max_key() -> Self;

    /// Converts a `Felt` to the key type, assuming it is a valid key.
    fn from_felt(felt: Felt) -> Self;

    /// Returns a request covering the full key range `[default, max_key]` for the given context.
    fn initial_request(context: Self::Context) -> TreeRequest<Self> {
        TreeRequest { context, start: Self::default(), end: Self::max_key() }
    }

    /// Scans entries in `[start, end]` at `block_target` and returns `(state_diff, actual_end)`.
    ///
    /// `actual_end` is the inclusive end of the largest aligned Patricia subtree starting at the
    /// leaf `start` that is fully covered by the scan. The number of entries is ≤ `size_limit`.
    fn scan(
        reader: &StorageReader,
        request: &TreeRequest<Self>,
        block_target: BlockNumber,
        size_limit: usize,
    ) -> (StateDiff, Felt);
}

/// A request to populate a subtree of a particular tree.
///
/// `start` and `end` are both inclusive. For a valid subtree the range must satisfy:
/// `size = end - start + 1` is a power of two and `start % size == 0`.
struct TreeRequest<K: TreeKey> {
    context: K::Context,
    start: K,
    end: K,
}

impl TreeKey for StorageKey {
    type Context = ContractAddress;

    fn max_key() -> Self {
        Self::from_felt(MAX_PATRICIA_FELT)
    }

    fn from_felt(felt: Felt) -> Self {
        StorageKey(PatriciaKey::try_from(felt).expect("felt <= max_key() is a valid StorageKey"))
    }

    fn scan(
        reader: &StorageReader,
        request: &TreeRequest<Self>,
        block_target: BlockNumber,
        size_limit: usize,
    ) -> (StateDiff, Felt) {
        let txn = reader.begin_ro_txn().expect("Storage scan failed");
        let state_reader = txn.get_state_reader().expect("Storage scan failed");
        let raw_entries = state_reader
            .scan_storage_keys_for_contract(
                request.context,
                request.start,
                request.end,
                block_target,
                size_limit,
            )
            .expect("Storage scan failed");
        let (entries, actual_end) =
            shrink_to_actual_end(raw_entries, request.start, request.end, size_limit);
        let storage_map = entries
            .into_iter()
            .map(|(key, value)| (StarknetStorageKey(key), StarknetStorageValue(value)))
            .collect();
        (
            StateDiff {
                storage_updates: HashMap::from([(request.context, storage_map)]),
                ..Default::default()
            },
            actual_end,
        )
    }
}

impl TreeKey for ContractAddress {
    type Context = ();

    fn max_key() -> Self {
        Self::from_felt(MAX_PATRICIA_FELT)
    }

    fn from_felt(felt: Felt) -> Self {
        ContractAddress::try_from(felt).expect("felt <= max_key() is a valid ContractAddress")
    }

    /// Scans up to `size_limit / 2` contract-address-to-class-hash entries in
    /// `[request.start, request.end]`, then fetches the nonce for each scanned address (assuming
    /// every deployed contract has a nonce entry).
    ///
    /// Returns a `StateDiff` with `address_to_class_hash` and `address_to_nonce` populated, and
    /// the inclusive actual end of the largest aligned Patricia subtree covered by the scan.
    fn scan(
        reader: &StorageReader,
        request: &TreeRequest<Self>,
        block_target: BlockNumber,
        size_limit: usize,
    ) -> (StateDiff, Felt) {
        let txn = reader.begin_ro_txn().expect("Class hash scan failed");
        let state_reader = txn.get_state_reader().expect("Class hash scan failed");
        let raw_class_entries = state_reader
            .scan_contract_class_hashes_in_range(
                request.start,
                request.end,
                block_target,
                size_limit / 2,
            )
            .expect("Class hash scan failed");
        let (class_entries, actual_end) =
            shrink_to_actual_end(raw_class_entries, request.start, request.end, size_limit / 2);

        let state_number = StateNumber::unchecked_right_after_block(block_target);
        let address_to_nonce = class_entries
            .iter()
            .map(|(addr, _)| {
                let nonce = state_reader
                    .get_nonce_at(state_number, addr)
                    .expect("Nonce lookup failed")
                    .unwrap_or_default();
                (*addr, nonce)
            })
            .collect();

        let address_to_class_hash = class_entries.into_iter().collect();
        (StateDiff { address_to_class_hash, address_to_nonce, ..Default::default() }, actual_end)
    }
}

impl TreeKey for ClassHash {
    type Context = ();

    fn max_key() -> Self {
        ClassHash(Felt::MAX)
    }

    fn from_felt(felt: Felt) -> Self {
        ClassHash(felt)
    }

    fn scan(
        reader: &StorageReader,
        request: &TreeRequest<Self>,
        block_target: BlockNumber,
        size_limit: usize,
    ) -> (StateDiff, Felt) {
        let txn = reader.begin_ro_txn().expect("Compiled class hash scan failed");
        let state_reader = txn.get_state_reader().expect("Compiled class hash scan failed");
        let raw_entries = state_reader
            .scan_compiled_class_hashes_in_range(
                request.start,
                request.end,
                block_target,
                size_limit,
            )
            .expect("Compiled class hash scan failed");
        let (entries, actual_end) =
            shrink_to_actual_end(raw_entries, request.start, request.end, size_limit);
        let class_hash_to_compiled_class_hash = entries
            .into_iter()
            .map(|(class_hash, compiled_class_hash)| {
                (class_hash, CommitterCompiledClassHash(compiled_class_hash.0))
            })
            .collect();
        (StateDiff { class_hash_to_compiled_class_hash, ..Default::default() }, actual_end)
    }
}

/// Shared mutable state: the DB and running `StateRoots` threaded across all commits.
struct CommitState<S: Storage = ApolloStorage> {
    committer_db: IndexDb<S>,
    state_roots: StateRoots,
    num_commits: usize,
}

/// Divides a Felt by 2 (right-shift by 1).
fn shr_one(felt: Felt) -> Felt {
    felt.floor_div(&NonZeroFelt::TWO)
}

/// Commits a state diff to the shared `CommitState`.
async fn commit_state_diff<S: Storage + Send>(
    state_diff: StateDiff,
    commit_state: &Mutex<CommitState<S>>,
) {
    if state_diff == StateDiff::default() {
        return;
    }
    let mut guard = commit_state.lock().await;
    let input = Input {
        state_diff,
        initial_read_context: IndexDbReadContext::create_empty(),
        config: ReaderConfig::default(),
    };
    let (filled_forest, deleted_nodes) =
        CommitBlockImpl::commit_block(input, &mut guard.committer_db, &mut NoMeasurements)
            .await
            .expect("Failed to commit batch");
    guard
        .committer_db
        .write_with_metadata(&filled_forest, HashMap::new(), deleted_nodes)
        .await
        .expect("Failed to write forest to storage");
    guard.state_roots = filled_forest.state_roots();
    guard.num_commits += 1;
    info!(
        "Committed batch {} (contracts root: {}, classes root: {})",
        guard.num_commits,
        guard.state_roots.contracts_trie_root_hash.0.to_hex_string(),
        guard.state_roots.classes_trie_root_hash.0.to_hex_string(),
    );
}

/// Processes a tree request: scans a subtree, commits it, then either recurses linearly or
/// splits the remaining range into two parallel sub-requests.
///
/// This is a boxed future because it recurses.
#[allow(dead_code)]
fn process_request<K: TreeKey, S: Storage + Send + 'static>(
    reader: Arc<StorageReader>,
    request: TreeRequest<K>,
    block_target: BlockNumber,
    size_limit: usize,
    commit_state: Arc<Mutex<CommitState<S>>>,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    Box::pin(async move {
        let (state_diff, actual_end) = K::scan(&reader, &request, block_target, size_limit);
        commit_state_diff(state_diff, &commit_state).await;

        let start_felt: Felt = request.start.into();
        let end_felt: Felt = request.end.into();
        let remaining_start = actual_end + Felt::ONE;
        if actual_end >= end_felt {
            return;
        }

        let request_range_size = end_felt - start_felt + Felt::ONE;
        let covered = actual_end - start_felt + Felt::ONE;

        // If we covered ≤ 1/4 of the range, split the remainder and run both halves in parallel.
        if covered <= shr_one(shr_one(request_range_size)) {
            let mid = start_felt + shr_one(end_felt - start_felt);
            let left = TreeRequest {
                context: request.context.clone(),
                start: K::from_felt(remaining_start),
                end: K::from_felt(mid),
            };
            let right = TreeRequest {
                context: request.context,
                start: K::from_felt(mid + Felt::ONE),
                end: request.end,
            };
            tokio::join!(
                process_request(
                    Arc::clone(&reader),
                    left,
                    block_target,
                    size_limit,
                    Arc::clone(&commit_state)
                ),
                process_request(
                    Arc::clone(&reader),
                    right,
                    block_target,
                    size_limit,
                    Arc::clone(&commit_state)
                ),
            );
        } else {
            // Continue linearly with the next chunk.
            let next_request = TreeRequest {
                context: request.context,
                start: K::from_felt(remaining_start),
                end: request.end,
            };
            process_request(reader, next_request, block_target, size_limit, commit_state).await;
        }
    })
}

/// Returns the first contract address >= `start_addr` that has any storage entry,
/// or `None` if no such contract exists.
#[expect(dead_code)]
fn find_next_storage_contract(
    reader: &StorageReader,
    start_addr: ContractAddress,
) -> StorageResult<Option<ContractAddress>> {
    let txn = reader.begin_ro_txn()?;
    let state_reader = txn.get_state_reader()?;
    state_reader.find_next_storage_contract(start_addr)
}
