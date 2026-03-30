use std::collections::HashMap;

use apollo_storage::state::StateStorageReader;
use apollo_storage::StorageReader;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, ContractAddress, PatriciaKey, MAX_PATRICIA_FELT};
use starknet_api::state::{StateNumber, StorageKey};
use starknet_committer::block_committer::input::{
    StarknetStorageKey,
    StarknetStorageValue,
    StateDiff,
};
use starknet_committer::patricia_merkle_tree::types::CompiledClassHash as CommitterCompiledClassHash;
use starknet_types_core::felt::Felt;

#[cfg(test)]
#[path = "snap_sync_test.rs"]
mod snap_sync_test;

/// Returns the number of trailing zero bits in the binary representation of `felt`.
/// Equivalent to the largest `k` such that `2^k` divides `felt`.
/// Panics if `felt` is zero.
fn trailing_zeros(felt: Felt) -> u64 {
    assert!(felt != Felt::ZERO, "trailing_zeros called with zero");
    let mut count = 0u64;
    for &byte in felt.to_bytes_be().iter().rev() {
        if byte == 0 {
            count += 8;
        } else {
            count += u64::from(byte.trailing_zeros());
            break;
        }
    }
    count
}

/// Given a subtree `start` and the felt of the last key seen (`last_key`), returns the inclusive
/// end of the largest valid Patricia subtree rooted at `start` that doesn't exceed `last_key`.
///
/// A valid Patricia subtree of size `2^k` requires `start % 2^k == 0`, so the size is capped
/// by the alignment of `start`.
fn compute_actual_end(start: Felt, last_key: Felt) -> Felt {
    let covered = last_key - start + Felt::ONE;
    // The number of bits needed to represent numbers in the range [0, covered).
    let covered_bit_width = u64::try_from(covered.bits()).expect("covered bits fits in u64") - 1;
    let exponent = if start == Felt::ZERO {
        covered_bit_width
    } else {
        covered_bit_width.min(trailing_zeros(start))
    };
    start + Felt::TWO.pow(exponent) - Felt::ONE
}

/// Filters `entries` to those within the actual aligned Patricia subtree end, and returns that end.
///
/// - Fewer than `limit` entries: all entries are returned with the full requested `end`.
/// - Exactly `limit` entries: entries are filtered to `[start, actual_end]` where `actual_end` is
///   the inclusive end of the largest aligned Patricia subtree rooted at `start` that fits within
///   the last key returned.
///
/// Panics if `limit` is 0.
fn shrink_to_actual_end<K: TreeKey, V>(
    mut entries: Vec<(K, V)>,
    start: K,
    end: K,
    limit: usize,
) -> (Vec<(K, V)>, Felt) {
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

/// Identifies which Patricia trie a request targets.
/// Trait for Patricia trie key types used in `TreeRequest`.
///
/// `Context` carries any per-request metadata needed by `scan`.
#[allow(dead_code)]
trait TreeKey: Copy + Into<Felt> + Send + Sync + 'static {
    type Context: Clone + Send + Sync + 'static;

    /// Returns the inclusive maximum key value for this key type.
    fn max_key() -> Self;

    /// Converts a `Felt` to the key type, assuming it is a valid key.
    fn from_felt(felt: Felt) -> Self;

    /// Scans entries in `[start, end]` at `block_target` and returns `(state_diff, actual_end)`.
    ///
    /// `actual_end` is the inclusive end of the largest aligned Patricia subtree rooted at `start`
    /// that is fully covered by the scan. The number of entries is ≤ `size_limit`.
    fn scan(
        reader: &StorageReader,
        request: &TreeRequest<Self>,
        block_target: BlockNumber,
        size_limit: usize,
    ) -> (StateDiff, Felt);
}

/// A request to populate a subtree of a particular trie.
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
