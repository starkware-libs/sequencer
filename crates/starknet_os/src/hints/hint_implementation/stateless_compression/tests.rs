use std::cmp::min;

use assert_matches::assert_matches;
use num_bigint::BigUint;
use num_integer::Integer;
use num_traits::ToPrimitive;
use rstest::rstest;
use starknet_types_core::felt::Felt;

use super::utils::{
    get_bucket_offsets,
    get_n_elms_per_felt,
    pack_usize_in_felts,
    BitLength,
    BitsArray,
    BucketElement,
    BucketElement125,
    BucketElement31,
    BucketElement62,
    BucketElementTrait,
    Buckets,
    CompressionSet,
    N_UNIQUE_BUCKETS,
};
use crate::hints::error::OsHintError;

// Utils

pub fn unpack_felts<const LENGTH: usize>(
    compressed: &[Felt],
    n_elms: usize,
) -> Vec<BitsArray<LENGTH>> {
    let n_elms_per_felt = BitLength::min_bit_length(LENGTH).unwrap().n_elems_in_felt();
    let mut result = Vec::with_capacity(n_elms);

    for felt in compressed {
        let n_packed_elms = min(n_elms_per_felt, n_elms - result.len());
        for chunk in felt.to_bits_le()[0..n_packed_elms * LENGTH].chunks_exact(LENGTH) {
            result.push(BitsArray(chunk.try_into().unwrap()));
        }
    }

    result
}

pub fn unpack_felts_to_usize(compressed: &[Felt], n_elms: usize, elm_bound: u32) -> Vec<usize> {
    let n_elms_per_felt = get_n_elms_per_felt(elm_bound);
    let elm_bound_as_big = BigUint::from(elm_bound);
    let mut result = Vec::with_capacity(n_elms);

    for felt in compressed {
        let mut remaining = felt.to_biguint();
        let n_packed_elms = min(n_elms_per_felt, n_elms - result.len());
        for _ in 0..n_packed_elms {
            let (new_remaining, value) = remaining.div_rem(&elm_bound_as_big);
            result.push(value.to_usize().unwrap());
            remaining = new_remaining;
        }
    }

    result
}

// Tests

#[rstest]
#[case::zero([false; 10], Felt::ZERO)]
#[case::thousand(
    [false, false, false, true, false, true, true, true, true, true],
    Felt::from(0b_0000_0011_1110_1000_u16),
)]
fn test_bits_array(#[case] expected: [bool; 10], #[case] felt: Felt) {
    assert_eq!(BitsArray::<10>::try_from(felt).unwrap().0, expected);
}

#[rstest]
#[case::max_fits(16, Felt::from(0xFFFF_u16))]
#[case::overflow(252, Felt::MAX)]
fn test_overflow_bits_array(#[case] n_bits_felt: usize, #[case] felt: Felt) {
    let error = BitsArray::<10>::try_from(felt).unwrap_err();
    assert_matches!(
        error, OsHintError::StatelessCompressionOverflow { n_bits, .. } if n_bits == n_bits_felt
    );
}

#[test]
fn test_pack_and_unpack() {
    let felts = [
        Felt::from(34_u32),
        Felt::from(0_u32),
        Felt::from(11111_u32),
        Felt::from(1034_u32),
        Felt::from(3404_u32),
    ];
    let bucket: Vec<_> =
        felts.into_iter().map(|f| BucketElement125::try_from(f).unwrap()).collect();
    let packed = BucketElement125::pack_in_felts(&bucket);
    let unpacked = unpack_felts(packed.as_ref(), bucket.len());
    assert_eq!(bucket, unpacked);
}

#[test]
fn test_buckets() {
    let mut buckets = Buckets::new();
    buckets.add(BucketElement::BucketElement31(BucketElement31::try_from(Felt::ONE).unwrap()));
    buckets.add(BucketElement::BucketElement62(BucketElement62::try_from(Felt::TWO).unwrap()));
    let bucket62_3 =
        BucketElement::BucketElement62(BucketElement62::try_from(Felt::THREE).unwrap());
    buckets.add(bucket62_3.clone());

    assert_eq!(buckets.get_element_index(&bucket62_3), Some(&1_usize));
    assert_eq!(buckets.lengths(), [0, 0, 0, 2, 1, 0]);
}

#[test]
fn test_usize_pack_and_unpack() {
    let nums = vec![34, 0, 11111, 1034, 3404, 16, 32, 127, 129, 128];
    let elm_bound = 12345;
    let packed = pack_usize_in_felts(&nums, elm_bound);
    let unpacked = unpack_felts_to_usize(packed.as_ref(), nums.len(), elm_bound);
    assert_eq!(nums, unpacked);
}

#[test]
fn test_get_bucket_offsets() {
    let lengths = vec![2, 3, 5];
    let offsets = get_bucket_offsets(&lengths);
    assert_eq!(offsets, [0, 2, 5]);
}

#[rstest]
#[case::unique_values(
    vec![
        Felt::from(42),                    // < 15 bits
        Felt::from(12833943439439439_u64), // 54 bits
        Felt::from(1283394343),            // 31 bits
    ],
    [0, 0, 0, 1, 1, 1],
    0,
    vec![],
)]
#[case::repeated_values(
    vec![
        Felt::from(43),
        Felt::from(42),
        Felt::from(42),
        Felt::from(42),
    ],
    [0, 0, 0, 0, 0, 2],
    2,
    vec![1, 1],
)]
#[case::edge_bucket_values(
    vec![
        Felt::from((BigUint::from(1_u8) << 15) - 1_u8),
        Felt::from(BigUint::from(1_u8) << 15),
        Felt::from((BigUint::from(1_u8) << 31) - 1_u8),
        Felt::from(BigUint::from(1_u8) << 31),
        Felt::from((BigUint::from(1_u8) << 62) - 1_u8),
        Felt::from(BigUint::from(1_u8) << 62),
        Felt::from((BigUint::from(1_u8) << 83) - 1_u8),
        Felt::from(BigUint::from(1_u8) << 83),
        Felt::from((BigUint::from(1_u8) << 125) - 1_u8),
        Felt::from(BigUint::from(1_u8) << 125),
        Felt::MAX,
    ],
    [2, 2, 2, 2, 2, 1],
    0,
    vec![],
)]
fn test_update_with_unique_values(
    #[case] values: Vec<Felt>,
    #[case] expected_unique_lengths: [usize; N_UNIQUE_BUCKETS],
    #[case] expected_n_repeating_values: usize,
    #[case] expected_repeating_value_pointers: Vec<usize>,
) {
    let compression_set = CompressionSet::new(&values);
    assert_eq!(expected_unique_lengths, compression_set.get_unique_value_bucket_lengths());
    assert_eq!(expected_n_repeating_values, compression_set.n_repeating_values());
    assert_eq!(expected_repeating_value_pointers, compression_set.get_repeating_value_pointers());
}
