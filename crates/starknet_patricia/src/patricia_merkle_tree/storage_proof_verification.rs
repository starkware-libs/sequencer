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

#[derive(Debug, PartialEq, Eq)]
pub struct ProofIndexMaps {
    pub hash_by_index: HashMap<NodeIndex, HashOutput>,
    pub parent_by_index: HashMap<NodeIndex, NodeIndex>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProofVerificationError {
    #[error("missing leaf at index {index:?}")]
    MissingLeaf { index: NodeIndex },
    #[error("missing inner node on path at index {index:?}")]
    MissingNode { index: NodeIndex },
    #[error("hash mismatch at index {index:?}: proof {proof_value:?}, actual {actual:?}")]
    HashMismatch { index: NodeIndex, proof_value: HashOutput, actual: HashOutput },
}

/// Verifies that `preimages` form valid Patricia paths from each accessed leaf to `root_hash`.
pub fn verify_patricia_proof<L: Leaf, TH: TreeHashFunction<L>>(
    root_hash: HashOutput,
    preimages: &PreimageMap,
    leaf_indices: &[NodeIndex],
    leaf_hashes: &HashMap<NodeIndex, HashOutput>,
) -> Result<(), ProofVerificationError> {
    if leaf_indices.is_empty() || root_hash == HashOutput::ROOT_OF_EMPTY_TREE {
        return Ok(());
    }

    let maps = build_proof_index_maps::<L, TH>(root_hash, preimages)?;
    verify_leaf_paths(&maps, preimages, root_hash, leaf_indices, leaf_hashes)
}

/// Builds `index -> hash` and `index -> parent` maps by expanding a Patricia proof from the root.
///
/// Verifies that the supplied hashes are consistent with the supplied preimages. This is, if the
/// proof contains a path from the root to some leaf, it is a valid proof for that leaf.
pub fn build_proof_index_maps<L: Leaf, TH: TreeHashFunction<L>>(
    root_hash: HashOutput,
    preimages: &PreimageMap,
) -> Result<ProofIndexMaps, ProofVerificationError> {
    let mut hash_by_index = HashMap::from([(NodeIndex::ROOT, root_hash)]);
    let mut parent_by_index = HashMap::new();
    let mut queue = VecDeque::from([NodeIndex::ROOT]);

    while let Some(index) = queue.pop_front() {
        let hash = hash_by_index[&index];

        // Either a leaf (sibling or accessed) or a child of a node in `preimages` which is not
        // necessarily required to verify the proof.
        let Some(preimage) = preimages.get(&hash) else {
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
                register_child(
                    &mut hash_by_index,
                    &mut parent_by_index,
                    &mut queue,
                    index,
                    left_index,
                    binary.left_data,
                );
                register_child(
                    &mut hash_by_index,
                    &mut parent_by_index,
                    &mut queue,
                    index,
                    right_index,
                    binary.right_data,
                );
            }
            Preimage::Edge(edge) => {
                let bottom_index = edge.path_to_bottom.bottom_index(index);
                register_child(
                    &mut hash_by_index,
                    &mut parent_by_index,
                    &mut queue,
                    index,
                    bottom_index,
                    edge.bottom_data,
                );
            }
        }
    }

    Ok(ProofIndexMaps { hash_by_index, parent_by_index })
}

/// Verifies that `preimages` contains a path from the root to each of the leaves in `leaf_indices`.
fn verify_leaf_paths(
    maps: &ProofIndexMaps,
    preimages: &PreimageMap,
    root_hash: HashOutput,
    leaf_indices: &[NodeIndex],
    leaf_hashes: &HashMap<NodeIndex, HashOutput>,
) -> Result<(), ProofVerificationError> {
    for leaf_index in leaf_indices {
        let actual_leaf_hash = leaf_hashes.get(leaf_index).ok_or_else(|| {
            panic!("leaf_hashes must contain every requested leaf index; missing {leaf_index:?}")
        })?;

        match maps.hash_by_index.get(leaf_index) {
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

        let mut current_index = *leaf_index;
        while current_index != NodeIndex::ROOT {
            let parent_index = maps
                .parent_by_index
                .get(&current_index)
                .copied()
                .ok_or(ProofVerificationError::MissingNode { index: current_index })?;
            let parent_hash = maps
                .hash_by_index
                .get(&parent_index)
                .ok_or(ProofVerificationError::MissingNode { index: current_index })?;
            let preimage = preimages
                .get(parent_hash)
                .ok_or(ProofVerificationError::MissingNode { index: current_index })?;
            if !is_valid_child(current_index, parent_index, preimage) {
                return Err(ProofVerificationError::MissingNode { index: current_index });
            }
            current_index = parent_index;
        }
    }

    match maps.hash_by_index.get(&NodeIndex::ROOT) {
        Some(proof_value) if *proof_value == root_hash => Ok(()),
        Some(proof_value) => Err(ProofVerificationError::HashMismatch {
            index: NodeIndex::ROOT,
            proof_value: *proof_value,
            actual: root_hash,
        }),
        None => Err(ProofVerificationError::MissingNode { index: NodeIndex::ROOT }),
    }
}

fn register_child(
    hash_by_index: &mut HashMap<NodeIndex, HashOutput>,
    parent_by_index: &mut HashMap<NodeIndex, NodeIndex>,
    queue: &mut VecDeque<NodeIndex>,
    parent_index: NodeIndex,
    child_index: NodeIndex,
    child_hash: HashOutput,
) {
    if hash_by_index.contains_key(&child_index) || parent_by_index.contains_key(&child_index) {
        unreachable!("child index {child_index:?} already registered");
    }
    hash_by_index.insert(child_index, child_hash);
    parent_by_index.insert(child_index, parent_index);
    queue.push_back(child_index);
}

fn is_valid_child(child_index: NodeIndex, parent_index: NodeIndex, preimage: &Preimage) -> bool {
    match preimage {
        Preimage::Binary(_) => {
            let [left_index, right_index] = parent_index.get_children_indices();
            child_index == left_index || child_index == right_index
        }
        Preimage::Edge(edge) => child_index == edge.path_to_bottom.bottom_index(parent_index),
    }
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
