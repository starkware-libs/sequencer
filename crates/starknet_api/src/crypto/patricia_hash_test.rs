use starknet_types_core::felt::Felt;
use starknet_types_core::hash::Poseidon;

use super::calculate_root;
use crate::felt;

#[test]
fn test_patricia_regression() {
    let root =
        calculate_root::<Poseidon>(vec![Felt::from(1_u8), Felt::from(2_u8), Felt::from(3_u8)]);
    let expected_root = felt!("0x3b5cc7f1292eb3847c3f902d048a7e5dc7702d1c191ccd17c2d33f797e6fc32");
    assert_eq!(root, expected_root);
}

#[test]
fn test_edge_patricia_regression() {
    let root = calculate_root::<Poseidon>(vec![Felt::from(1_u8)]);
    let expected_root = felt!("0x7752582c54a42fe0fa35c40f07293bb7d8efe90e21d8d2c06a7db52d7d9b7e1");
    assert_eq!(root, expected_root);
}

#[test]
fn test_binary_patricia_regression() {
    let root = calculate_root::<Poseidon>(vec![Felt::from(1_u8), Felt::from(2_u8)]);
    let expected_root = felt!("0x1c1ba983ee0a0de87d87d67ea3cbee7023aa65f6b7bcf71259f122ea3af80bf");
    assert_eq!(root, expected_root);
}
