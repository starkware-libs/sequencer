use starknet_api::core::ClassHash;
use starknet_types_core::felt::Felt;

use super::{compute_actual_end, shrink_to_actual_end, trailing_zeros};

fn ch(n: u64) -> ClassHash {
    ClassHash(Felt::from(n))
}

#[test]
fn test_trailing_zeros_powers_of_two() {
    assert_eq!(trailing_zeros(Felt::ONE), 0);
    assert_eq!(trailing_zeros(Felt::from(2u64)), 1);
    assert_eq!(trailing_zeros(Felt::from(4u64)), 2);
    assert_eq!(trailing_zeros(Felt::from(256u64)), 8);
}

#[test]
fn test_trailing_zeros_non_powers_of_two() {
    assert_eq!(trailing_zeros(Felt::from(3u64)), 0);
    assert_eq!(trailing_zeros(Felt::from(6u64)), 1);
    assert_eq!(trailing_zeros(Felt::from(12u64)), 2);
    assert_eq!(trailing_zeros(Felt::from(24u64)), 3);
    assert_eq!(trailing_zeros(Felt::from(255u64)), 0);
}

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

#[test]
fn test_shrink_to_actual_end_fewer_than_limit() {
    // Under the limit: all entries returned, end returned as-is.
    let entries = vec![(ch(0), ()), (ch(1), ()), (ch(2), ())];
    let (result, actual_end) = shrink_to_actual_end(entries.clone(), ch(0), ch(16), 4);
    assert_eq!(result, entries);
    assert_eq!(actual_end, Felt::from(16u64));
}

#[test]
fn test_shrink_to_actual_end_at_limit_truncates() {
    // start=0, last_key=4 → covered=5, subtree_size=4, actual_end=3 (inclusive); entry at key 4
    // is dropped.
    let entries = vec![(ch(0), ()), (ch(1), ()), (ch(2), ()), (ch(4), ())];
    let (result, actual_end) = shrink_to_actual_end(entries, ch(0), ch(8), 4);
    assert_eq!(result, vec![(ch(0), ()), (ch(1), ()), (ch(2), ())]);
    assert_eq!(actual_end, Felt::from(3u64));
}
