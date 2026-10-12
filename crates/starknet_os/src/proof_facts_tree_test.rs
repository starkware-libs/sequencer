use leaf_proof_format::PackedNode;
use rstest::rstest;
use starknet_types_core::felt::Felt;

use super::{
    flatten_proof_facts_tree,
    ProofFactsTreeError,
    FLAT_TREE_FOLD,
    FLAT_TREE_LEAF,
    FLAT_TREE_SELF_FOLD,
};
use crate::proof_fact_fold::{
    Blake2sDigestWords,
    LEAF_VERIFIER_CIRCUIT_HASH,
    MULTIVERIFIER_CIRCUIT_HASH,
};

const OTHER_CIRCUIT_HASH: Blake2sDigestWords = [1, 2, 3, 4, 5, 6, 7, 8];

/// A leaf node with the given circuit hash, as the proving side packs it.
fn leaf_node(circuit_hash: Blake2sDigestWords) -> PackedNode {
    PackedNode::leaf(circuit_hash, vec!["1".to_string()])
}

/// A multiverifier node over the given children.
fn fold_node(children: Vec<PackedNode>) -> PackedNode {
    PackedNode::Composite { circuit_hash: MULTIVERIFIER_CIRCUIT_HASH, subtasks: children }
}

fn flatten(packed_tree: &PackedNode) -> Result<Vec<Felt>, ProofFactsTreeError> {
    flatten_proof_facts_tree(
        packed_tree,
        &[OTHER_CIRCUIT_HASH, LEAF_VERIFIER_CIRCUIT_HASH],
        &[MULTIVERIFIER_CIRCUIT_HASH],
    )
}

fn felts(values: &[u8]) -> Vec<Felt> {
    values.iter().copied().map(Felt::from).collect()
}

/// The proving side's single-leaf tree: a self-fold of the leaf.
#[test]
fn test_flatten_single_leaf_tree() {
    assert_eq!(
        flatten(&fold_node(vec![leaf_node(LEAF_VERIFIER_CIRCUIT_HASH)])).unwrap(),
        felts(&[FLAT_TREE_SELF_FOLD, 0, FLAT_TREE_LEAF, 1])
    );
}

/// A three-leaf tree, as the proving side builds it: the first two leaves fold together, and the
/// third is carried up to fold with them at the root.
#[test]
fn test_flatten_tree_with_carried_leaf() {
    let packed_tree = fold_node(vec![
        fold_node(vec![leaf_node(LEAF_VERIFIER_CIRCUIT_HASH), leaf_node(OTHER_CIRCUIT_HASH)]),
        leaf_node(LEAF_VERIFIER_CIRCUIT_HASH),
    ]);
    assert_eq!(
        flatten(&packed_tree).unwrap(),
        felts(&[
            FLAT_TREE_FOLD,
            0,
            FLAT_TREE_FOLD,
            0,
            FLAT_TREE_LEAF,
            1,
            FLAT_TREE_LEAF,
            0,
            FLAT_TREE_LEAF,
            1,
        ])
    );
}

#[rstest]
#[case::unknown_leaf_verifier_hash(
    fold_node(vec![leaf_node(MULTIVERIFIER_CIRCUIT_HASH)]),
    ProofFactsTreeError::UnknownCircuitHash {
        circuit_hash: MULTIVERIFIER_CIRCUIT_HASH,
        role: "leaf verifier",
    },
)]
#[case::unknown_multiverifier_hash(
    PackedNode::Composite {
        circuit_hash: OTHER_CIRCUIT_HASH,
        subtasks: vec![leaf_node(OTHER_CIRCUIT_HASH), leaf_node(OTHER_CIRCUIT_HASH)],
    },
    ProofFactsTreeError::UnknownCircuitHash {
        circuit_hash: OTHER_CIRCUIT_HASH,
        role: "multiverifier",
    },
)]
#[case::self_fold_below_root(
    fold_node(vec![
        fold_node(vec![leaf_node(OTHER_CIRCUIT_HASH)]),
        leaf_node(OTHER_CIRCUIT_HASH),
    ]),
    ProofFactsTreeError::SelfFoldBelowRoot,
)]
fn test_flatten_rejects_invalid_trees(
    #[case] packed_tree: PackedNode,
    #[case] expected_error: ProofFactsTreeError,
) {
    assert_eq!(flatten(&packed_tree).unwrap_err(), expected_error);
}
