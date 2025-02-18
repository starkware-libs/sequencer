use std::cmp::min;

use num_bigint::BigUint;
use num_integer::Integer;
use num_traits::ToPrimitive;
use rstest::rstest;
use starknet_types_core::felt::Felt;

use super::utils::{pack_in_felts, SizedBitsVec};
use crate::hints::hint_implementation::stateless_compression::utils::{
    get_bucket_offsets,
    get_n_elms_per_felt,
    pack_usize_in_felts,
    UpperBound,
    MAX_N_BITS,
};

// Utils

pub fn unpack_felts(compressed: &[Felt], n_elms: usize, n_bits: usize) -> Vec<SizedBitsVec> {
    let n_elms_per_felt = get_n_elms_per_felt(UpperBound::NBits(n_bits));
    let mut result = Vec::with_capacity(n_elms);

    for felt in compressed {
        let n_packed_elms = min(n_elms_per_felt, n_elms - result.len());
        for chunk in felt.to_bits_le()[0..n_packed_elms * n_bits].chunks_exact(n_bits) {
            result.push(SizedBitsVec(chunk.to_vec()));
        }
    }

    result
}

pub fn unpack_felts_to_usize(compressed: &[Felt], n_elms: usize, elm_bound: usize) -> Vec<usize> {
    let n_elms_per_felt = get_n_elms_per_felt(UpperBound::BiggestNum(elm_bound));
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

#[test]
fn test_bits_n() {
    let expected = [false, false, false, true, false, true, true, true, true, true];
    assert_eq!(SizedBitsVec::from_felt(Felt::from(0b_0000_0011_1110_1000_u16), 10).0, expected);
}

#[rstest]
#[case(1, MAX_N_BITS)]
#[case(125, 2)]
#[case(83, 3)]
#[case(62, 4)]
#[case(31, 8)]
#[case(15, 16)]
fn test_get_n_elms_per_felt(#[case] input: usize, #[case] expected: usize) {
    assert_eq!(get_n_elms_per_felt(UpperBound::NBits(input)), expected);
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
    let n_bits = 125;
    let bucket: Vec<_> = felts.into_iter().map(|f| SizedBitsVec::from_felt(f, n_bits)).collect();
    let packed = pack_in_felts(&bucket, n_bits);
    let unpacked = unpack_felts(packed.as_ref(), bucket.len(), n_bits);
    assert_eq!(bucket, unpacked);
}

#[test]
fn test_usize_pack_and_unpack() {
    let nums = [34, 0, 11111, 1034, 3404];
    let elm_bound = 12345;
    let packed = pack_usize_in_felts(&nums, elm_bound);
    let unpacked = unpack_felts_to_usize(packed.as_ref(), nums.len(), elm_bound);
    assert_eq!(nums.to_vec(), unpacked);
}

#[test]
fn test_get_bucket_offsets() {
    let lengths = vec![2, 3, 5];
    let offsets = get_bucket_offsets(&lengths);
    assert_eq!(offsets, [0, 2, 5]);
}
