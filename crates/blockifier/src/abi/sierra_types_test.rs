use num_bigint::BigUint;
use starknet_types_core::felt::Felt;

use crate::abi::sierra_types::felt_to_u128;

#[test]
fn test_value_too_large_for_type() {
    // Happy flow.
    let n = 1991_u128;
    let n_as_felt = Felt::from(n);
    felt_to_u128(&n_as_felt).unwrap();

    // Value too large for type.
    let overflowed_u128: BigUint = BigUint::from(1_u8) << 128;
    let overflowed_u128_as_felt = Felt::from(overflowed_u128);
    let error = felt_to_u128(&overflowed_u128_as_felt).unwrap_err();
    assert_eq!(
        format!("{error}"),
        "Felt 340282366920938463463374607431768211456 is too big to convert to 'u128'."
    );
}
