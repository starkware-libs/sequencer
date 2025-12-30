use crate::merkle::*;

#[test]
fn test_empty_tree() {
    let tree = MerkleTree::new(&[]);
    assert_eq!(tree.leaf_count(), 0);
    assert_eq!(tree.root(), [0u8; 32]);
}

#[test]
fn test_single_leaf() {
    let data = vec![vec![1, 2, 3, 4]];
    let tree = MerkleTree::new(&data);
    assert_eq!(tree.leaf_count(), 1);

    let proof = tree.prove(0).unwrap();
    assert_eq!(proof.siblings.len(), 0);
    assert!(tree.verify(&tree.leaves()[0], &proof, 0));
}

#[test]
fn test_two_leaves() {
    let data = vec![vec![1, 2, 3], vec![4, 5, 6]];
    let tree = MerkleTree::new(&data);
    assert_eq!(tree.leaf_count(), 2);

    // Test proof for first leaf
    let proof0 = tree.prove(0).unwrap();
    assert_eq!(proof0.siblings.len(), 1);
    assert!(tree.verify(&tree.leaves()[0], &proof0, 0));

    // Test proof for second leaf
    let proof1 = tree.prove(1).unwrap();
    assert_eq!(proof1.siblings.len(), 1);
    assert!(tree.verify(&tree.leaves()[1], &proof1, 1));
}

#[test]
fn test_multiple_leaves() {
    let data: Vec<Vec<u8>> = (0..8).map(|i| vec![i]).collect();
    let tree = MerkleTree::new(&data);
    assert_eq!(tree.leaf_count(), 8);

    // Verify all leaves
    for i in 0..8 {
        let proof = tree.prove(i).unwrap();
        assert!(tree.verify(&tree.leaves()[i], &proof, i));

        // Proof should have log2(8) = 3 siblings
        assert_eq!(proof.siblings.len(), 3);
    }
}

#[test]
fn test_proof_verification_fails_for_wrong_leaf() {
    let data: Vec<Vec<u8>> = (0..4).map(|i| vec![i]).collect();
    let tree = MerkleTree::new(&data);

    let proof = tree.prove(0).unwrap();
    let wrong_leaf = tree.leaves()[1];

    // Should fail with wrong leaf
    assert!(!tree.verify(&wrong_leaf, &proof, 0));
}

#[test]
fn test_odd_number_of_leaves() {
    let data: Vec<Vec<u8>> = (0..7).map(|i| vec![i]).collect();
    let tree = MerkleTree::new(&data);
    assert_eq!(tree.leaf_count(), 7);

    // All proofs should still verify
    for i in 0..7 {
        let proof = tree.prove(i).unwrap();
        assert!(tree.verify(&tree.leaves()[i], &proof, i));
    }
}

#[test]
fn test_from_leaves() {
    let leaves: Vec<MerkleHash> = (0..4)
        .map(|i| {
            let mut hash = [0u8; 32];
            hash[0] = i;
            hash
        })
        .collect();

    let tree = MerkleTree::from_leaves(leaves.clone());
    assert_eq!(tree.leaf_count(), 4);

    for (i, leaf) in leaves.iter().enumerate() {
        let proof = tree.prove(i).unwrap();
        assert!(tree.verify(leaf, &proof, i));
    }
}

#[test]
fn test_deterministic_root() {
    let data: Vec<Vec<u8>> = vec![vec![1, 2, 3], vec![4, 5, 6], vec![7, 8, 9]];

    let tree1 = MerkleTree::new(&data);
    let tree2 = MerkleTree::new(&data);

    assert_eq!(tree1.root(), tree2.root());
}
