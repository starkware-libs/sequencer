use rstest::rstest;

use crate::merkle::*;

#[rstest]
#[case(1, 0, 0)]
#[case(2, 0, 1)]
#[case(2, 1, 1)]
#[case(3, 0, 2)]
#[case(3, 1, 2)]
#[case(3, 2, 1)]
#[case(4, 0, 2)]
#[case(4, 1, 2)]
#[case(4, 2, 2)]
#[case(4, 3, 2)]
#[case(5, 0, 3)]
#[case(5, 1, 3)]
#[case(5, 2, 3)]
#[case(5, 3, 3)]
#[case(5, 4, 1)]
#[case(6, 0, 3)]
#[case(6, 1, 3)]
#[case(6, 2, 3)]
#[case(6, 3, 3)]
#[case(6, 4, 2)]
#[case(6, 5, 2)]
#[case(7, 0, 3)]
#[case(7, 1, 3)]
#[case(7, 2, 3)]
#[case(7, 3, 3)]
#[case(7, 4, 3)]
#[case(7, 5, 3)]
#[case(7, 6, 2)]
fn test_merkle_proof_length(#[case] n: u8, #[case] leaf_index: usize, #[case] proof_length: usize) {
    let data: Vec<_> = (0..n).map(|i| vec![i]).collect();
    let tree = MerkleTree::new(&data);
    let proof = tree.prove(leaf_index).unwrap();
    assert_eq!(proof.siblings.len(), proof_length);
}

#[rstest]
#[case(1)]
#[case(2)]
#[case(3)]
#[case(4)]
#[case(5)]
#[case(6)]
#[case(7)]
fn test_merkle_proof_validity(#[case] n: u8) {
    let data: Vec<_> = (0..n).map(|i| vec![i]).collect();
    let tree = MerkleTree::new(&data);
    for (proof_index, data_chunk) in data.iter().enumerate() {
        let proof = tree.prove(proof_index).unwrap();
        let leaf_hash = MerkleTree::hash_leaf(data_chunk);
        assert!(tree.verify(&leaf_hash, &proof, proof_index).unwrap());
    }
}

#[rstest]
#[case(1)]
#[case(2)]
#[case(3)]
#[case(4)]
#[case(5)]
#[case(6)]
#[case(7)]
#[case(8)]
#[case(9)]
#[case(10)]
fn test_merkle_tampered_data_proof_invalidity(#[case] n: u8) {
    let data: Vec<_> = (0..n).map(|i| vec![i]).collect();
    let tree = MerkleTree::new(&data);
    for (proof_index, data_chunk) in data.iter().enumerate() {
        let proof = tree.prove(proof_index).unwrap();
        let mut tampered_data = data_chunk.clone();
        tampered_data.push(1);
        let leaf_hash = MerkleTree::hash_leaf(&tampered_data);
        assert!(
            !tree.verify(&leaf_hash, &proof, proof_index).unwrap(),
            "proof_index={proof_index}"
        );
    }
}
