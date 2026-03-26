use starknet_types_core::felt::Felt;

#[cfg(test)]
#[path = "snap_sync_test.rs"]
mod snap_sync_test;

/// Given a subtree `start` and the felt of the last key seen (`last_key`), returns the inclusive
/// end of the largest valid Patricia subtree rooted at `start` that doesn't exceed `last_key`.
///
/// A valid Patricia subtree of size `2^k` requires `start % 2^k == 0`, so the size is capped
/// by the alignment of `start`.
#[cfg_attr(not(test), expect(dead_code))]
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
