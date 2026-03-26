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
#[cfg_attr(not(test), expect(dead_code))]
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
