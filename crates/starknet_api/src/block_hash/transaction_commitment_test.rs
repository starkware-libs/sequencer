use starknet_types_core::felt::Felt;
use starknet_types_core::hash::Poseidon;

use super::TransactionLeafElement;
use crate::block_hash::transaction_commitment::{
    calculate_transaction_commitment,
    calculate_transaction_leaf,
};
use crate::core::TransactionCommitment;
use crate::felt;
use crate::transaction::fields::{TransactionHash, TransactionSignature};

#[test]
fn test_transaction_leaf_regression() {
    let transaction_leaf_elements = get_transaction_leaf_element();
    let expected_leaf = felt!("0x2f0d8840bcf3bc629598d8a6cc80cb7c0d9e52d93dab244bbf9cd0dca0ad082");

    assert_eq!(expected_leaf, calculate_transaction_leaf(&transaction_leaf_elements));
}

#[test]
fn test_transaction_leaf_without_signature_regression() {
    let transaction_leaf_elements = TransactionLeafElement {
        transaction_hash: TransactionHash(Felt::ONE),
        transaction_signature: TransactionSignature(vec![]),
    };
    let expected_leaf = felt!("0x579e8877c7755365d5ec1ec7d3a94a457eff5d1f40482bbe9729c064cdead2");

    assert_eq!(expected_leaf, calculate_transaction_leaf(&transaction_leaf_elements));
}

#[test]
fn test_transaction_commitment_regression() {
    let transaction_leaf_elements = get_transaction_leaf_element();
    let expected_root = felt!("0x0282b635972328bd1cfa86496fe920d20bd9440cd78ee8dc90ae2b383d664dcf");

    assert_eq!(
        TransactionCommitment(expected_root),
        calculate_transaction_commitment::<Poseidon>(&[
            transaction_leaf_elements.clone(),
            transaction_leaf_elements
        ])
    );
}

fn get_transaction_leaf_element() -> TransactionLeafElement {
    let transaction_hash = TransactionHash(Felt::ONE);
    let transaction_signature = TransactionSignature(vec![Felt::TWO, Felt::THREE]);
    TransactionLeafElement { transaction_hash, transaction_signature }
}
