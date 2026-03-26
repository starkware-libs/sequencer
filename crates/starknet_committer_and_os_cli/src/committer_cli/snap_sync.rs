use starknet_types_core::felt::Felt;

#[cfg(test)]
#[path = "snap_sync_test.rs"]
mod snap_sync_test;

/// Returns the largest power of two that is ≤ `n`.
/// Panics if `n` is zero.
#[allow(unused)]
fn prev_power_of_two(n: Felt) -> Felt {
    assert!(n != Felt::ZERO, "prev_power_of_two called with zero");
    let bytes = n.to_bytes_be();
    // Find the most-significant set bit.
    let mut result_bytes = [0u8; 32];
    for (byte_idx, &byte) in bytes.iter().enumerate() {
        if byte != 0 {
            let msb_pos = 7u32 - byte.leading_zeros();
            result_bytes[byte_idx] = 1u8 << msb_pos;
            return Felt::from_bytes_be_slice(&result_bytes);
        }
    }
    unreachable!("n was non-zero but all bytes were zero")
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

