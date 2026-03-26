use starknet_types_core::felt::Felt;

#[cfg(test)]
#[path = "snap_sync_test.rs"]
mod snap_sync_test;

/// Returns the largest power of two that is ≤ `n`.
/// Panics if `n` is zero.
#[allow(unused)]
fn prev_power_of_two(n: Felt) -> Felt {
    assert!(n != Felt::ZERO, "prev_power_of_two called with zero");
    let n_bits: u64 = n.bits().try_into().expect("n_bits of felt must fit in u64");
    Felt::TWO.pow(n_bits - 1)
}

/// Given a subtree `start` and the felt of the last key seen (`last_key`), returns the inclusive
/// end of the largest valid Patricia subtree rooted at `start` that contains `last_key`.
#[allow(unused)]
fn compute_actual_end(start: Felt, last_key: Felt) -> Felt {
    // covered = last_key - start + 1  (number of keys from start to last, inclusive)
    let covered = last_key - start + Felt::ONE;
    let subtree_size = prev_power_of_two(covered);
    start + subtree_size - Felt::ONE
}
