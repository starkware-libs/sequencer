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
#[allow(dead_code)]
fn compute_actual_end(start: Felt, last_key: Felt) -> Felt {
    // covered = last_key - start + 1  (number of keys from start to last, inclusive)
    let covered = last_key - start + Felt::ONE;
    let subtree_size = prev_power_of_two(covered);
    start + subtree_size - Felt::ONE
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
