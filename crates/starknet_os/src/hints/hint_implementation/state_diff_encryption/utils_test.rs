use starknet_types_core::felt::Felt;

use crate::hints::hint_implementation::state_diff_encryption::utils::{
    decrypt_state_diff,
    recover_y,
};

#[test]
fn test_decrypt_state_diff() {
    let private_key = Felt::from_hex_unchecked("0x123456789abcdef");
    let sn_public_key =
        Felt::from_hex_unchecked("0x8747394510780077664574649897481151382481072868806602");
    let encrypted_symmetric_key = Felt::from_hex_unchecked("0x1111111111111111");
    let encrypted_state_diff = vec![
        Felt::from_hex_unchecked("0x2222222222222222"),
        Felt::from_hex_unchecked("0x3333333333333333"),
    ];

    let result = decrypt_state_diff(
        private_key,
        sn_public_key,
        encrypted_symmetric_key,
        &encrypted_state_diff,
    );

    assert_eq!(result.len(), encrypted_state_diff.len());
    assert_ne!(result[0], encrypted_state_diff[0]);
    assert_ne!(result[1], encrypted_state_diff[1]);
}

#[test]
fn test_recover_y() {
    let g = starknet_types_core::curve::AffinePoint::generator();
    for i in 1..16 {
        let p = &g * Felt::from(i);
        let x = p.x();
        let y = recover_y(x);
        assert_eq!(y * y, p.y() * p.y());
    }
}

#[test]
#[should_panic]
fn test_recover_y_not_on_curve() {
    let x = Felt::from(18);
    recover_y(x);
}
