use rstest::rstest;
use starknet_types_core::felt::Felt;

use super::utils::BitsArray;

#[rstest]
#[case::zero([false; 10], Felt::ZERO)]
#[case::thousand(
    [false, false, false, true, false, true, true, true, true, true],
    Felt::from(0b_0000_0011_1110_1000_u16),
)]
#[case::max_fits([true; 10], Felt::from(0xFFFF_u16))]
#[case::should_panic::overflow([true; 10], Felt::MAX)]
fn test_bits_array(#[case] expected: [bool; 10], #[case] felt: Felt) {
    assert_eq!(BitsArray::<10>::try_from(felt).unwrap().0, expected);
}
