use std::cmp::min;

use assert_matches::assert_matches;
use num_bigint::BigUint;
use num_integer::Integer;
use num_traits::ToPrimitive;
use rstest::rstest;
use starknet_types_core::felt::Felt;

use super::utils::{
    compress,
    felt_from_bits_le,
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
    COMPRESSION_VERSION,
    HEADER_ELM_BOUND,
    N_UNIQUE_BUCKETS,
    TOTAL_N_BUCKETS,
};
use crate::hints::error::OsHintError;

const HEADER_LEN: usize = 1 + 1 + TOTAL_N_BUCKETS;
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

pub fn unpack_felts_to_usize(compressed: &[Felt], n_elms: usize, elm_bound: usize) -> Vec<usize> {
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

/// Decompresses the given compressed data.
pub fn decompress(compressed: &mut impl Iterator<Item = Felt>) -> Vec<Felt> {
    fn unpack_chunk<const LENGTH: usize>(
        compressed: &mut impl Iterator<Item = Felt>,
        n_elms: usize,
    ) -> Vec<Felt> {
        let n_elms_per_felt = BitLength::min_bit_length(LENGTH).unwrap().n_elems_in_felt();
        let n_packed_felts = n_elms.div_ceil(n_elms_per_felt);
        let compressed_chunk: Vec<_> = compressed.take(n_packed_felts).collect();
        unpack_felts(&compressed_chunk, n_elms)
            .into_iter()
            .map(|bits: BitsArray<LENGTH>| felt_from_bits_le(&bits.0).unwrap())
            .collect()
    }

    fn unpack_chunk_to_usize(
        compressed: &mut impl Iterator<Item = Felt>,
        n_elms: usize,
        elm_bound: usize,
    ) -> Vec<usize> {
        let n_elms_per_felt = get_n_elms_per_felt(elm_bound);
        let n_packed_felts = n_elms.div_ceil(n_elms_per_felt);

        let compressed_chunk: Vec<_> = compressed.take(n_packed_felts).collect();
        unpack_felts_to_usize(&compressed_chunk, n_elms, elm_bound)
    }

    let header = unpack_chunk_to_usize(compressed, HEADER_LEN, HEADER_ELM_BOUND);
    let version = &header[0];
    assert!(version == &usize::from(COMPRESSION_VERSION), "Unsupported compression version.");

    let data_len = &header[1];
    let unique_value_bucket_lengths: Vec<usize> = header[2..2 + N_UNIQUE_BUCKETS].to_vec();
    let n_repeating_values = &header[2 + N_UNIQUE_BUCKETS];

    let mut unique_values = Vec::new();
    unique_values.extend(unpack_chunk::<252>(compressed, unique_value_bucket_lengths[0]));
    unique_values.extend(unpack_chunk::<125>(compressed, unique_value_bucket_lengths[1]));
    unique_values.extend(unpack_chunk::<83>(compressed, unique_value_bucket_lengths[2]));
    unique_values.extend(unpack_chunk::<62>(compressed, unique_value_bucket_lengths[3]));
    unique_values.extend(unpack_chunk::<31>(compressed, unique_value_bucket_lengths[4]));
    unique_values.extend(unpack_chunk::<15>(compressed, unique_value_bucket_lengths[5]));

    let repeating_value_pointers =
        unpack_chunk_to_usize(compressed, *n_repeating_values, N_UNIQUE_BUCKETS);

    let repeating_values: Vec<_> =
        repeating_value_pointers.iter().map(|ptr| unique_values[*ptr]).collect();

    let mut all_values = unique_values;
    all_values.extend(repeating_values);

    let bucket_index_per_elm: Vec<usize> =
        unpack_chunk_to_usize(compressed, *data_len, TOTAL_N_BUCKETS);

    let all_bucket_lengths: Vec<usize> =
        unique_value_bucket_lengths.into_iter().chain([*n_repeating_values]).collect();

    let bucket_offsets = get_bucket_offsets(&all_bucket_lengths);

    let mut bucket_offset_iterators: Vec<_> = bucket_offsets;

    let mut result = Vec::new();
    for bucket_index in bucket_index_per_elm {
        let offset = &mut bucket_offset_iterators[bucket_index];
        let value = all_values[*offset];
        *offset += 1;
        result.push(value);
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
    let nums = vec![34, 0, 11111, 1034, 3404];
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

#[test]
fn test_update_with_unique_values() {
    let values = vec![
        Felt::from(42),                    // < 15 bits
        Felt::from(12833943439439439_u64), // 54 bits
        Felt::from(1283394343),            // 31 bits
    ];
    let compression_set = CompressionSet::new(&values);

    let unique_lengths = compression_set.get_unique_value_bucket_lengths();
    assert_eq!(unique_lengths, [0, 0, 0, 1, 1, 1]);
}

#[test]
fn test_update_with_repeated_values() {
    let values = vec![Felt::from(42), Felt::from(42)];
    let compression_set = CompressionSet::new(&values);

    let unique_lengths = compression_set.get_unique_value_bucket_lengths();
    assert_eq!(unique_lengths, [0, 0, 0, 0, 0, 1]);
    assert_eq!(compression_set.n_repeating_values(), 1);
}

#[test]
fn test_get_repeating_value_pointers_with_repeated_values() {
    let values = vec![Felt::from(42), Felt::from(42)];
    let compression_set = CompressionSet::new(&values);

    let pointers = compression_set.get_repeating_value_pointers();
    assert_eq!(pointers, [0]);
}

#[test]
fn test_get_repeating_value_pointers_with_no_repeated_values() {
    let values = vec![Felt::from(42), Felt::from(128)];
    let compression_set = CompressionSet::new(&values);

    let pointers = compression_set.get_repeating_value_pointers();
    assert!(pointers.is_empty());
}

// These values are calculated by importing the module and running the compression method
// ```py
// # import compress from compression
// def main() -> int:
//     print(compress([2,3,1]))
//     return 0
// ```
#[rstest]
#[case::single_value_1(vec![1u32], vec!["0x100000000000000000000000000000100000", "0x1", "0x5"])]
#[case::single_value_2(vec![2u32], vec!["0x100000000000000000000000000000100000", "0x2", "0x5"])]
#[case::single_value_3(vec![10u32], vec!["0x100000000000000000000000000000100000", "0xA", "0x5"])]
#[case::two_values(vec![1u32, 2], vec!["0x200000000000000000000000000000200000", "0x10001", "0x28"])]
#[case::three_values(vec![2u32, 3, 1], vec!["0x300000000000000000000000000000300000", "0x40018002", "0x11d"])]
#[case::four_values(vec![1u32, 2, 3, 4], vec!["0x400000000000000000000000000000400000", "0x8000c0010001", "0x7d0"])]
#[case::extracted_kzg_example(vec![1u32, 1, 6, 1991, 66, 0], vec!["0x10000500000000000000000000000000000600000", "0x841f1c0030001", "0x0", "0x17eff"])]

fn test_compress_decompress(#[case] input: Vec<u32>, #[case] expected: Vec<&str>) {
    let data: Vec<_> = input.into_iter().map(Felt::from).collect();

    let compressed = compress(&data);

    let expected: Vec<_> = expected.iter().map(|s| Felt::from_hex_unchecked(s)).collect();

    assert_eq!(compressed, expected);

    let decompressed = decompress(&mut compressed.into_iter());

    assert_eq!(decompressed, data);
}
