use std::cmp::min;

use num_bigint::BigUint;
use num_integer::Integer;
use num_traits::ToPrimitive;
use rstest::rstest;
use starknet_types_core::felt::Felt;

use super::utils::{pack_in_felts, SizedBitsVec, TOTAL_N_BUCKETS};
use crate::hints::hint_implementation::stateless_compression::utils::{
    compress,
    get_bucket_offsets,
    get_n_elms_per_felt,
    pack_usize_in_felts,
    CompressionSet,
    UpperBound,
    COMPRESSION_VERSION,
    HEADER_ELM_BOUND,
    MAX_N_BITS,
    N_BITS_PER_BUCKET,
};

const HEADER_LEN: usize = 1 + 1 + TOTAL_N_BUCKETS;
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

/// Decompresses the given compressed data.
pub fn decompress(compressed: &mut impl Iterator<Item = Felt>) -> Vec<Felt> {
    fn unpack_chunk(
        compressed: &mut impl Iterator<Item = Felt>,
        n_elms: usize,
        n_bits: usize,
    ) -> Vec<SizedBitsVec> {
        let n_elms_per_felt = get_n_elms_per_felt(UpperBound::NBits(n_bits));
        let n_packed_felts = n_elms.div_ceil(n_elms_per_felt);

        let compressed_chunk: Vec<_> = compressed.take(n_packed_felts).collect();
        unpack_felts(&compressed_chunk, n_elms, n_bits)
    }

    fn unpack_chunk_to_usize(
        compressed: &mut impl Iterator<Item = Felt>,
        n_elms: usize,
        elm_bound: usize,
    ) -> Vec<usize> {
        let n_elms_per_felt = get_n_elms_per_felt(UpperBound::BiggestNum(elm_bound));
        let n_packed_felts = n_elms.div_ceil(n_elms_per_felt);

        let compressed_chunk: Vec<_> = compressed.take(n_packed_felts).collect();
        unpack_felts_to_usize(&compressed_chunk, n_elms, elm_bound)
    }

    let header = unpack_chunk_to_usize(compressed, HEADER_LEN, HEADER_ELM_BOUND);
    let version = &header[0];
    assert!(version == &usize::from(COMPRESSION_VERSION), "Unsupported compression version.");

    let data_len = &header[1];
    let unique_value_bucket_lengths: Vec<usize> = header[2..2 + N_BITS_PER_BUCKET.len()].to_vec();
    let n_repeating_values = &header[2 + N_BITS_PER_BUCKET.len()];

    let mut unique_values = Vec::new();
    for (&length, &n_bits) in unique_value_bucket_lengths.iter().zip(&N_BITS_PER_BUCKET) {
        unique_values.extend(unpack_chunk(compressed, length, n_bits));
    }

    let repeating_value_pointers =
        unpack_chunk_to_usize(compressed, *n_repeating_values, unique_values.len());

    let repeating_values: Vec<_> =
        repeating_value_pointers.iter().map(|ptr| unique_values[*ptr].clone()).collect();

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
        let value = all_values[*offset].clone();
        *offset += 1;
        result.push(value.into());
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

#[test]
fn test_update_with_unique_values() {
    let mut compression_set = CompressionSet::new(&[8, 16, 32]);
    let values = vec![Felt::from(42), Felt::from(12833943439439439_u64), Felt::from(1283394343)];

    compression_set.update(&values);

    let unique_lengths = compression_set.get_unique_value_bucket_lengths();
    assert_eq!(unique_lengths, vec![1, 0, 1]);
}

#[test]
fn test_update_with_repeated_values() {
    let mut compression_set = CompressionSet::new(&[8, 16, 32]);
    let values = vec![Felt::from(42), Felt::from(42)];

    compression_set.update(&values);

    let unique_lengths = compression_set.get_unique_value_bucket_lengths();
    assert_eq!(unique_lengths, vec![1, 0, 0]);
    assert_eq!(compression_set.get_repeating_value_bucket_length(), 1);
}

#[test]
fn test_get_repeating_value_pointers_with_repeated_values() {
    let mut compression_set = CompressionSet::new(&[8, 16, 32]);
    let values = vec![Felt::from(42), Felt::from(42)];

    compression_set.update(&values);
    compression_set.finalize();

    let pointers = compression_set.get_repeating_value_pointers();
    assert_eq!(pointers, [0]);
}

#[test]
fn test_get_repeating_value_pointers_with_no_repeated_values() {
    let mut compression_set = CompressionSet::new(&[8, 16, 32]);
    let values = vec![Felt::from(42), Felt::from(128)];

    compression_set.update(&values);
    compression_set.finalize();

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
