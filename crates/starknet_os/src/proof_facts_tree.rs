//! Encodes the proving side's packed proof tree (`packed_output` of
//! `stwo_run_and_prove_recursive_tree`) as the flat felt array the Cairo proof-facts tree walk
//! reads.

use leaf_proof_format::PackedNode;
use starknet_types_core::felt::Felt;

use crate::proof_fact_fold::Blake2sDigestWords;

#[cfg(test)]
#[path = "proof_facts_tree_test.rs"]
mod proof_facts_tree_test;

/// The tag of a leaf in a flattened tree: the leaf verifier's proof of one transaction's proof
/// facts.
pub const FLAT_TREE_LEAF: u8 = 0;
/// The tag of a fold in a flattened tree: a multiverifier proof of two children.
pub const FLAT_TREE_FOLD: u8 = 1;
/// The tag of a self-fold in a flattened tree: a multiverifier proof of one child in both of its
/// slots. Only the single-leaf tree's root is one.
pub const FLAT_TREE_SELF_FOLD: u8 = 2;

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum ProofFactsTreeError {
    #[error("Circuit hash {circuit_hash:?} is not an allowed {role} circuit hash.")]
    UnknownCircuitHash { circuit_hash: Blake2sDigestWords, role: &'static str },
    #[error("A self-fold node is only allowed at the root of the tree.")]
    SelfFoldBelowRoot,
    #[error("Unsupported packed node: {0}.")]
    UnsupportedNode(String),
}

/// Flattens `packed_tree` into pre-order felts, replacing each node's circuit hash with its index
/// in the allowed circuit hashes of its role:
/// - a leaf is `[FLAT_TREE_LEAF, leaf verifier hash index]`;
/// - a fold is `[FLAT_TREE_FOLD, multiverifier hash index, ...left, ...right]`;
/// - a self-fold, only at the root, is `[FLAT_TREE_SELF_FOLD, multiverifier hash index, ...child]`.
///
/// Leaf output preimages are left out: the Cairo walk takes each leaf's preimage from the proof
/// facts of the transaction it stands for.
pub fn flatten_proof_facts_tree(
    packed_tree: &PackedNode,
    leaf_verifier_circuit_hashes: &[Blake2sDigestWords],
    multiverifier_circuit_hashes: &[Blake2sDigestWords],
) -> Result<Vec<Felt>, ProofFactsTreeError> {
    let allowed_circuit_hashes =
        AllowedCircuitHashes { leaf_verifier_circuit_hashes, multiverifier_circuit_hashes };
    let mut flat_tree = Vec::new();
    match classify_node(packed_tree)? {
        ProofTreeNode::SelfFold { circuit_hash, child } => {
            flat_tree.push(Felt::from(FLAT_TREE_SELF_FOLD));
            flat_tree.push(allowed_circuit_hashes.multiverifier_index(circuit_hash)?);
            flatten_node(child, &allowed_circuit_hashes, &mut flat_tree)?;
        }
        _ => flatten_node(packed_tree, &allowed_circuit_hashes, &mut flat_tree)?,
    }
    Ok(flat_tree)
}

/// Appends the flattening of a non-root node to `flat_tree`.
fn flatten_node(
    node: &PackedNode,
    allowed_circuit_hashes: &AllowedCircuitHashes<'_>,
    flat_tree: &mut Vec<Felt>,
) -> Result<(), ProofFactsTreeError> {
    match classify_node(node)? {
        ProofTreeNode::Leaf { circuit_hash } => {
            flat_tree.push(Felt::from(FLAT_TREE_LEAF));
            flat_tree.push(allowed_circuit_hashes.leaf_verifier_index(circuit_hash)?);
        }
        ProofTreeNode::Fold { circuit_hash, left, right } => {
            flat_tree.push(Felt::from(FLAT_TREE_FOLD));
            flat_tree.push(allowed_circuit_hashes.multiverifier_index(circuit_hash)?);
            flatten_node(left, allowed_circuit_hashes, flat_tree)?;
            flatten_node(right, allowed_circuit_hashes, flat_tree)?;
        }
        ProofTreeNode::SelfFold { .. } => return Err(ProofFactsTreeError::SelfFoldBelowRoot),
    }
    Ok(())
}

/// A packed node, by its role in the proof tree.
enum ProofTreeNode<'a> {
    Leaf { circuit_hash: &'a Blake2sDigestWords },
    Fold { circuit_hash: &'a Blake2sDigestWords, left: &'a PackedNode, right: &'a PackedNode },
    SelfFold { circuit_hash: &'a Blake2sDigestWords, child: &'a PackedNode },
}

/// A leaf is a `Composite` over its single `Plain` preimage, a fold is a `Composite` over two
/// children, and a self-fold is a `Composite` over one `Composite` child.
fn classify_node(node: &PackedNode) -> Result<ProofTreeNode<'_>, ProofFactsTreeError> {
    let PackedNode::Composite { circuit_hash, subtasks } = node else {
        return Err(ProofFactsTreeError::UnsupportedNode(format!("{node:?}")));
    };
    match subtasks.as_slice() {
        [PackedNode::Plain { .. }] => Ok(ProofTreeNode::Leaf { circuit_hash }),
        [child @ PackedNode::Composite { .. }] => {
            Ok(ProofTreeNode::SelfFold { circuit_hash, child })
        }
        [left, right] => Ok(ProofTreeNode::Fold { circuit_hash, left, right }),
        _ => Err(ProofFactsTreeError::UnsupportedNode(format!("{node:?}"))),
    }
}

struct AllowedCircuitHashes<'a> {
    leaf_verifier_circuit_hashes: &'a [Blake2sDigestWords],
    multiverifier_circuit_hashes: &'a [Blake2sDigestWords],
}

impl AllowedCircuitHashes<'_> {
    fn leaf_verifier_index(
        &self,
        circuit_hash: &Blake2sDigestWords,
    ) -> Result<Felt, ProofFactsTreeError> {
        find_index(self.leaf_verifier_circuit_hashes, circuit_hash, "leaf verifier")
    }

    fn multiverifier_index(
        &self,
        circuit_hash: &Blake2sDigestWords,
    ) -> Result<Felt, ProofFactsTreeError> {
        find_index(self.multiverifier_circuit_hashes, circuit_hash, "multiverifier")
    }
}

fn find_index(
    allowed_circuit_hashes: &[Blake2sDigestWords],
    circuit_hash: &Blake2sDigestWords,
    role: &'static str,
) -> Result<Felt, ProofFactsTreeError> {
    allowed_circuit_hashes
        .iter()
        .position(|allowed_circuit_hash| allowed_circuit_hash == circuit_hash)
        .map(Felt::from)
        .ok_or(ProofFactsTreeError::UnknownCircuitHash { circuit_hash: *circuit_hash, role })
}
