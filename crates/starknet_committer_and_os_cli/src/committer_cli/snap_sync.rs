use apollo_storage::StorageReader;
use starknet_api::block::BlockNumber;
use starknet_committer::block_committer::input::StateDiff;
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
