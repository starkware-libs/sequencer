use starknet_types_core::felt::Felt;

use crate::hints::hint_implementation::stateless_compression::utils::SizedBitsVec;


#[test]
fn test_bits_n() {
    let expected = [false, false, false, true, false, true, true, true, true, true];
    assert_eq!(SizedBitsVec::from_felt(Felt::from(0b_0000_0011_1110_1000_u16), 10).0, expected);
}
