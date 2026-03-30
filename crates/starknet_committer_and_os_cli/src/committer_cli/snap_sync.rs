use apollo_storage::StorageReader;
use starknet_api::block::BlockNumber;
use starknet_committer::block_committer::input::StateDiff;
use starknet_types_core::felt::Felt;

#[cfg(test)]
#[path = "snap_sync_test.rs"]
mod snap_sync_test;

/// Returns the largest power of two that is ≤ `n`.
/// Panics if `n` is zero.
fn prev_power_of_two(n: Felt) -> Felt {
    assert!(n != Felt::ZERO, "prev_power_of_two called with zero");
    let n_bits: u64 = n.bits().try_into().expect("n_bits of felt must fit in u64");
    Felt::TWO.pow(n_bits - 1)
}

/// Given a subtree `start` and the felt of the last key seen (`last_key`), returns the inclusive
/// end of the largest valid Patricia subtree rooted at `start` that contains `last_key`.
fn compute_actual_end(start: Felt, last_key: Felt) -> Felt {
    // covered = last_key - start + 1  (number of keys from start to last, inclusive)
    let covered = last_key - start + Felt::ONE;
    let subtree_size = prev_power_of_two(covered);
    start + subtree_size - Felt::ONE
}

/// Filters `entries` to those within the actual aligned Patricia subtree end, and returns that end.
///
/// - Fewer than `limit` entries: all entries are returned with the full requested `end`.
/// - Exactly `limit` entries: entries are filtered to `[start, actual_end]` where `actual_end` is
///   the inclusive end of the largest aligned Patricia subtree rooted at `start` that fits within
///   the last key returned.
///
/// Panics if `limit` is 0.
#[allow(dead_code)]
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
#[allow(dead_code)]
struct TreeRequest<K: TreeKey> {
    context: K::Context,
    start: K,
    end: K,
}
