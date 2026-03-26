use starknet_types_core::felt::Felt;

use super::compute_actual_end;

#[test]
fn test_compute_actual_end_single_element() {
    // start == last_key: covered=1, subtree_size=1, actual_end=start
    assert_eq!(compute_actual_end(Felt::ZERO, Felt::ZERO), Felt::ZERO);
    assert_eq!(compute_actual_end(Felt::from(5u64), Felt::from(5u64)), Felt::from(5u64));
}

#[test]
fn test_compute_actual_end_covered_is_exact_power_of_two() {
    // start=0, last_key=3: covered=4, subtree_size=4, actual_end=3
    assert_eq!(compute_actual_end(Felt::ZERO, Felt::from(3u64)), Felt::from(3u64));
    // start=0, last_key=7: covered=8, subtree_size=8, actual_end=7
    assert_eq!(compute_actual_end(Felt::ZERO, Felt::from(7u64)), Felt::from(7u64));
}

#[test]
fn test_compute_actual_end_covered_is_not_power_of_two() {
    // start=0, last_key=4: covered=5, subtree_size=4, actual_end=3
    assert_eq!(compute_actual_end(Felt::ZERO, Felt::from(4u64)), Felt::from(3u64));
    // start=0, last_key=6: covered=7, subtree_size=4, actual_end=3
    assert_eq!(compute_actual_end(Felt::ZERO, Felt::from(6u64)), Felt::from(3u64));
    // start=0, last_key=14: covered=15, subtree_size=8, actual_end=7
    assert_eq!(compute_actual_end(Felt::ZERO, Felt::from(14u64)), Felt::from(7u64));
}

#[test]
fn test_compute_actual_end_non_zero_start() {
    // start=8, last_key=12: covered=5, subtree_size=4, actual_end=8+4-1=11
    assert_eq!(compute_actual_end(Felt::from(8u64), Felt::from(12u64)), Felt::from(11u64));
    // start=8, last_key=15: covered=8, subtree_size=8, actual_end=8+8-1=15
    assert_eq!(compute_actual_end(Felt::from(8u64), Felt::from(15u64)), Felt::from(15u64));
    // start=8, last_key=16: covered=9, subtree_size=8, actual_end=8+8-1=15
    assert_eq!(compute_actual_end(Felt::from(8u64), Felt::from(16u64)), Felt::from(15u64));
}

#[test]
fn test_compute_actual_end_unaligned_start() {
    // Alignment of 12 is 4, so the actual end is 12 + 4 - 1 = 15.
    assert_eq!(compute_actual_end(Felt::from(12u64), Felt::from(31u64)), Felt::from(15u64));
    // Alignment of 6 is 2, so the actual end is 6 + 2 - 1 = 7.
    assert_eq!(compute_actual_end(Felt::from(6u64), Felt::from(15u64)), Felt::from(7u64));
    // Alignment of 12 is 4, but the last key = 14 < 12 + 4 - 1 = 15.
    // So the actual end is determined by the last key.
    assert_eq!(compute_actual_end(Felt::from(12u64), Felt::from(14u64)), Felt::from(13u64));
}
