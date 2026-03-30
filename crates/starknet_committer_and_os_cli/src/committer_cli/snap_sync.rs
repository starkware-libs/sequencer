use apollo_storage::StorageReader;
use starknet_api::block::BlockNumber;
use starknet_committer::block_committer::input::StateDiff;
use starknet_types_core::felt::Felt;

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
#[expect(dead_code)]
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
trait TreeKey: Copy + Into<Felt> + Send + Sync + 'static {
    type Context: Clone + Send + Sync + 'static;

    /// Converts a `Felt` to the key type, assuming it is a valid key.
    fn from_felt(felt: Felt) -> Self;

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
#[allow(dead_code)]
struct TreeRequest<K: TreeKey> {
    context: K::Context,
    start: K,
    end: K,
}
