use sha3::{Digest, Keccak256};
use starknet_types_core::felt::Felt;

use super::CONSTRUCTOR_ENTRY_POINT_SELECTOR;

#[test]
fn test_constructor_selector() {
    let mut keccak = Keccak256::default();
    keccak.update(b"constructor");
    let mut constructor_bytes: [u8; 32] = keccak.finalize().into();
    constructor_bytes[0] &= 0b00000011_u8; // Discard the six MSBs.
    let constructor_felt = Felt::from_bytes_be(&constructor_bytes);
    assert_eq!(constructor_felt, CONSTRUCTOR_ENTRY_POINT_SELECTOR);
}
