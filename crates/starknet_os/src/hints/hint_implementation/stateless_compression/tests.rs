use std::collections::HashSet;

use assert_matches::assert_matches;
use num_bigint::BigUint;
use rstest::rstest;
use starknet_types_core::felt::Felt;

use super::utils::{
    compress,
    get_bucket_offsets,
    get_n_elms_per_felt,
    pack_usize_in_felts,
    BitsArray,
    BucketElement,
    BucketElement125,
    BucketElement31,
    BucketElement62,
    BucketElementTrait,
    Buckets,
    CompressionSet,
    N_UNIQUE_BUCKETS,
    TOTAL_N_BUCKETS,
};
use crate::hints::error::OsHintError;
use crate::hints::hint_implementation::stateless_compression::utils::{
    decompress,
    unpack_felts,
    unpack_felts_to_usize,
};

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

// These values are calculated by importing the module and running the compression method
// ```py
// # import compress from compression
// def main() -> int:
//     print(compress([2,3,1]))
//     return 0
// ```
#[rstest]
#[case::single_value_1(vec!["0x1"], vec!["0x100000000000000000000000000000100000", "0x1", "0x5"])]
#[case::single_value_2(vec!["0x2"], vec!["0x100000000000000000000000000000100000", "0x2", "0x5"])]
#[case::single_value_3(vec!["0xa"], vec!["0x100000000000000000000000000000100000", "0xA", "0x5"])]
#[case::two_values(vec!["0x1", "0x2"], vec!["0x200000000000000000000000000000200000", "0x10001", "0x28"])]
#[case::three_values(vec!["0x2", "0x3", "0x1"], vec!["0x300000000000000000000000000000300000", "0x40018002", "0x11d"])]
#[case::four_values(vec!["0x1", "0x2", "0x3", "0x4"], vec!["0x400000000000000000000000000000400000", "0x8000c0010001", "0x7d0"])]
#[case::extracted_kzg_example(vec!["0x1", "0x1", "0x6", "0x7c7", "0x42", "0x0"], vec!["0x10000500000000000000000000000000000600000", "0x841f1c0030001", "0x0", "0x17eff"])]
#[case::many_buckets(
    vec![
        "0x4",
        "0x2",
        "0x12",
        "0x0",
        "0x8d",
        "0x3b28019ccfdbd30ffc65951d94bb85c9e2b8434111a000b5afd533ce65f57a4",
        "0x8b",
        "0x3e761c56282df5f70f25fc154e6b3ac9335b7fc41538ecb9276c77a77ee42b4",
        "0x8a",
        "0x7e2c7f8fd5d138ac47de629f64c7a2c732b2fd5863bd4b7a3df13a9143313e2",
        "0x8c",
        "0x88",
        "0xa",
        "0x8a",
        "0x8af9fb61c",
        "0x87",
        "0xfffffffffffffffffffff7506049e4",
        "0x89",
        "0xc02",
        "0x8c",
        "0x7",
        "0x7c6f280b97046e3b87f91b905721b07cb7a3c0c5bc3e7415542d334de7c15b6",
        "0x8b",
        "0x7075626c69635f6b6579",
        "0x1",
        "0x7c6f280b97046e3b87f91b905721b07cb7a3c0c5bc3e7415542d334de7c15b6",
        "0x3ccabeeed5a2f8b62b97ade270f3c7276c38f2d88c7260ba080d8185c018d37"
    ],
    vec![
        "0x40000f00000000010000100001000050001b00000",
        "0x3b28019ccfdbd30ffc65951d94bb85c9e2b8434111a000b5afd533ce65f57a4",
        "0x3e761c56282df5f70f25fc154e6b3ac9335b7fc41538ecb9276c77a77ee42b4",
        "0x7e2c7f8fd5d138ac47de629f64c7a2c732b2fd5863bd4b7a3df13a9143313e2",
        "0x7c6f280b97046e3b87f91b905721b07cb7a3c0c5bc3e7415542d334de7c15b6",
        "0x3ccabeeed5a2f8b62b97ade270f3c7276c38f2d88c7260ba080d8185c018d37",
        "0xfffffffffffffffffffff7506049e4",
        "0x7075626c69635f6b6579",
        "0x8af9fb61c",
        "0x40038c020112021c005008801180228045808d000000480010004",
        "0xaad9",
        "0x1ec63d26ccd3d600757"
    ])]

fn test_compress_decompress(#[case] input: Vec<&str>, #[case] expected: Vec<&str>) {
    let data: Vec<_> = input.into_iter().map(Felt::from_hex_unchecked).collect();
    let compressed = compress(&data);
    let expected: Vec<_> = expected.iter().map(|s| Felt::from_hex_unchecked(s)).collect();
    assert_eq!(compressed, expected);

    let decompressed = decompress(&mut compressed.into_iter());
    assert_eq!(decompressed, data);
}

#[rstest]
#[case::no_values(
    vec![],
    0, // No buckets.
    None,
)]
#[case::single_value_1(
    vec![Felt::from(7777777)],
    1, // A single bucket with one value.
    Some(300), // 1 header, 1 value, 1 pointer
)]
#[case::large_duplicates(
    vec![Felt::from(BigUint::from(2_u8).pow(250)); 100],
    1, // Should remove duplicated values.
    Some(5),
)]
#[case::small_values(
    (0..0x8000).map(Felt::from).collect(),
    2048, // = 2**15/(251/15), as all elements are packed in the 15-bits bucket.
    Some(7),
)]
#[case::mixed_buckets(
    (0..252).map(|i| Felt::from(BigUint::from(2_u8).pow(i))).collect(),
    1 + 2 + 8 + 7 + 21 + 127, // All buckets are involved here.
    Some(67), // More than half of the values are in the biggest (252-bit) bucket.
)]
fn test_compression_length(
    #[case] data: Vec<Felt>,
    #[case] expected_unique_values_packed_length: usize,
    #[case] expected_compression_percents: Option<usize>,
) {
    let compressed = compress(&data);

    let n_unique_values = data.iter().collect::<HashSet<_>>().len();
    let n_repeated_values = data.len() - n_unique_values;
    let expected_repeated_value_pointers_packed_length =
        n_repeated_values.div_ceil(get_n_elms_per_felt(u32::try_from(n_unique_values).unwrap()));
    let expected_bucket_indices_packed_length =
        data.len().div_ceil(get_n_elms_per_felt(u32::try_from(TOTAL_N_BUCKETS).unwrap()));

    assert_eq!(
        compressed.len(),
        1 + expected_unique_values_packed_length
            + expected_repeated_value_pointers_packed_length
            + expected_bucket_indices_packed_length
    );

    if let Some(expected_compression_percents_val) = expected_compression_percents {
        assert_eq!(100 * compressed.len() / data.len(), expected_compression_percents_val);
    }
    assert_eq!(data, decompress(&mut compressed.into_iter()));
}
