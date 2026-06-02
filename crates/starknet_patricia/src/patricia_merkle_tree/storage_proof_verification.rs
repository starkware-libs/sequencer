use std::collections::{HashMap, VecDeque};

use starknet_api::hash::HashOutput;
use thiserror::Error;

use crate::patricia_merkle_tree::node_data::inner_node::{
    BinaryData,
    EdgeData,
    NodeData,
    Preimage,
    PreimageMap,
};
use crate::patricia_merkle_tree::node_data::leaf::Leaf;
use crate::patricia_merkle_tree::types::NodeIndex;
use crate::patricia_merkle_tree::updated_skeleton_tree::hash_function::TreeHashFunction;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProofVerificationError {
    #[error("missing leaf at index {index:?}")]
    MissingLeaf { index: NodeIndex },
    #[error("hash mismatch at index {index:?}: proof {proof_value:?}, actual {actual:?}")]
    HashMismatch { index: NodeIndex, proof_value: HashOutput, actual: HashOutput },
    #[error("duplicate parent for index {index:?}")]
    DuplicateParent { index: NodeIndex },
}

/// Verifies that `preimages` form valid Patricia paths from each accessed leaf to `root_hash`.
pub fn verify_patricia_proof<L: Leaf, TH: TreeHashFunction<L>>(
    root_hash: HashOutput,
    preimages: &PreimageMap,
    requested_leaves: &HashMap<NodeIndex, HashOutput>,
) -> Result<(), ProofVerificationError> {
    if requested_leaves.is_empty() {
        return Ok(());
    }

    let hash_by_index = build_proof_index_maps::<L, TH>(root_hash, preimages)?;

    // Once an index-->hash map is built, it suffices to verify that the expected leaf hashes are
    // present in the map, since we already verified the validity of the path from the root to the
    // leaf during the map construction.
    for (leaf_index, actual_leaf_hash) in requested_leaves {
        match hash_by_index.get(leaf_index) {
            None => return Err(ProofVerificationError::MissingLeaf { index: *leaf_index }),
            Some(proof_value) if *proof_value != *actual_leaf_hash => {
                return Err(ProofVerificationError::HashMismatch {
                    index: *leaf_index,
                    proof_value: *proof_value,
                    actual: *actual_leaf_hash,
                });
            }
            Some(_) => {}
        }
    }

    Ok(())
}

/// Builds an `index -> hash` map by expanding a Patricia proof from the root.
///
/// Verifies that the supplied hashes are consistent with the supplied preimages.
pub fn build_proof_index_maps<L: Leaf, TH: TreeHashFunction<L>>(
    root_hash: HashOutput,
    preimages: &PreimageMap,
) -> Result<HashMap<NodeIndex, HashOutput>, ProofVerificationError> {
    let mut hash_by_index = HashMap::from([(NodeIndex::ROOT, root_hash)]);
    let mut queue = VecDeque::from([NodeIndex::ROOT]);

    while let Some(index) = queue.pop_front() {
        let hash = hash_by_index[&index];

        let Some(preimage) = preimages.get(&hash) else {
            // We reach a leaf or a child-node of a node in `preimages` that is supposedly not
            // required in the proof (e.g. sibling of a node with no requested leaves in
            // its subtree).
            continue;
        };

        let computed_hash = TH::compute_node_hash(&preimage_to_node_data::<L>(preimage));
        if hash != computed_hash {
            return Err(ProofVerificationError::HashMismatch {
                index,
                proof_value: hash,
                actual: computed_hash,
            });
        }

        match preimage {
            Preimage::Binary(binary) => {
                let [left_index, right_index] = index.get_children_indices();
                register_child(&mut hash_by_index, &mut queue, left_index, binary.left_data)?;
                register_child(&mut hash_by_index, &mut queue, right_index, binary.right_data)?;
            }
            Preimage::Edge(edge) => {
                let bottom_index = edge.path_to_bottom.bottom_index(index);
                register_child(&mut hash_by_index, &mut queue, bottom_index, edge.bottom_data)?;
            }
        }
    }

    Ok(hash_by_index)
}

fn register_child(
    hash_by_index: &mut HashMap<NodeIndex, HashOutput>,
    queue: &mut VecDeque<NodeIndex>,
    child_index: NodeIndex,
    child_hash: HashOutput,
) -> Result<(), ProofVerificationError> {
    if hash_by_index.contains_key(&child_index) {
        return Err(ProofVerificationError::DuplicateParent { index: child_index });
    }
    hash_by_index.insert(child_index, child_hash);
    queue.push_back(child_index);
    Ok(())
}

fn preimage_to_node_data<L: Leaf>(preimage: &Preimage) -> NodeData<L, HashOutput> {
    match preimage {
        Preimage::Binary(binary) => NodeData::Binary(BinaryData {
            left_data: binary.left_data,
            right_data: binary.right_data,
        }),
        Preimage::Edge(edge) => NodeData::Edge(EdgeData {
            bottom_data: edge.bottom_data,
            path_to_bottom: edge.path_to_bottom,
        }),
    }
}
