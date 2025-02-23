use std::cmp::min;

use assert_matches::assert_matches;
use rstest::rstest;
use starknet_types_core::felt::Felt;

use super::utils::{
    BitLength,
    BitsArray,
    BucketElement,
    BucketElement31,
    BucketElement62,
    BucketElement125,
    BucketElementTrait,
    Buckets,
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
    let packed = BucketElement125::pack_in_felts(&bucket).unwrap();
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
