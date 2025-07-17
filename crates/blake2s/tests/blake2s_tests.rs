use blake2s::encode_felt252_data_and_calc_blake_hash;
use rstest::rstest;
use starknet_types_core::felt::Felt;

/// Test the encode_felt252_data_and_calc_blake_hash function
/// with the same result as the [Cairo v0.14](https://github.com/starkware-libs/cairo-lang/releases/tag/v0.14.0)
#[rstest]
#[case::empty(vec![], "874258848688468311465623299960361657518391155660316941922502367727700287818")]
#[case::boundary_small_felt(vec![Felt::from((1u64 << 63) - 1)], "94160078030592802631039216199460125121854007413180444742120780261703604445")]
#[case::boundary_at_2_63(vec![Felt::from(1u64 << 63)], "318549634615606806810268830802792194529205864650702991817600345489579978482")]
#[case::very_large_felt(vec![Felt::from_hex("0x800000000000011000000000000000000000000000000000000000000000000").unwrap()], "3505594194634492896230805823524239179921427575619914728883524629460058657521")]
#[case::mixed_small_large(vec![Felt::from(42), Felt::from(1u64 << 63), Felt::from(1337)], "1127477916086913892828040583976438888091205536601278656613505514972451246501")]
fn test_encode_felt252_data_and_calc_blake_hash(
    #[case] input: Vec<Felt>,
    #[case] expected_result: &str,
) {
    let result = encode_felt252_data_and_calc_blake_hash(&input);
    let expected = Felt::from_dec_str(expected_result).unwrap();
    assert_eq!(
        result, expected,
        "rust_implementation: {result:?} != cairo_implementation: {expected:?}"
    );
}
