use rstest::rstest;

use crate::merkle::*;

#[rstest]
#[case(1, 0)]
#[case(2, 1)]
#[case(3, 2)]
#[case(4, 2)]
#[case(5, 3)]
fn test_merkle_proof_length(#[case] n: u8, #[case] proof_length: usize) {
    let data: Vec<_> = (0..n).map(|i| vec![i]).collect();
    let tree = MerkleTree::new(&data);
    for proof_index in 0..n.into() {
        let proof = tree.prove(proof_index).unwrap();
        assert_eq!(proof.siblings.len(), proof_length);
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
