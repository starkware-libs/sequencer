use std::marker::PhantomData;

use crate::hash::hash_trait::{HashFunction, HashInputPair, HashOutput};
use crate::patricia_merkle_tree::filled_node::{BinaryData, LeafData, NodeData};
use crate::types::Felt;

#[cfg(test)]
#[path = "types_test.rs"]
pub mod types_test;

pub(crate) trait TreeHashFunction<L: LeafDataTrait, H: HashFunction> {
    /// Computes the hash of given node data.
    fn compute_node_hash(node_data: &NodeData<L>) -> HashOutput;
}

pub(crate) struct TreeHashFunctionImpl<H: HashFunction> {
    _hash_function: PhantomData<H>,
}

/// Implementation of TreeHashFunction.
// TODO(Aner, 11/4/25): Implement the function for LeafData::StorageValue and LeafData::StateTreeTuple
// TODO(Aner, 11/4/24): Verify the correctness of the implementation.
impl<H: HashFunction> TreeHashFunction<LeafData, H> for TreeHashFunctionImpl<H> {
    fn compute_node_hash(node_data: &NodeData<LeafData>) -> HashOutput {
        match node_data {
            NodeData::Binary(BinaryData {
                left_hash,
                right_hash,
            }) => H::compute_hash(HashInputPair(left_hash.0, right_hash.0)),
            NodeData::Edge(EdgeData {
                bottom_hash: hash_output,
                path_to_bottom: PathToBottom { path, length },
            }) => HashOutput(
                H::compute_hash(HashInputPair(hash_output.0, path.0)).0 + Felt::from(length.0),
            ),
            NodeData::Leaf(leaf_data) => match leaf_data {
                LeafData::StorageValue(_) => todo!(),
                LeafData::CompiledClassHash(compiled_class_hash) => {
                    HashOutput(compiled_class_hash.0)
                }
                LeafData::StateTreeTuple { .. } => {
                    todo!()
                }
            },
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct NodeIndex(pub Felt);

#[allow(dead_code)]
impl NodeIndex {
    pub(crate) fn root_index() -> NodeIndex {
        NodeIndex(Felt::ONE)
    }

    pub(crate) fn compute_bottom_index(
        index: NodeIndex,
        path_to_bottom: PathToBottom,
    ) -> NodeIndex {
        let PathToBottom { path, length } = path_to_bottom;
        NodeIndex(index.0 * Felt::TWO.pow(length.0) + path.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct EdgePath(pub Felt);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct EdgePathLength(pub u8);

#[allow(dead_code)]
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct TreeHeight(pub u8);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PathToBottom {
    pub path: EdgePath,
    pub length: EdgePathLength,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct EdgeData {
    pub(crate) bottom_hash: HashOutput,
    pub(crate) path_to_bottom: PathToBottom,
}

pub(crate) trait LeafDataTrait {
    /// Returns true if leaf is empty.
    fn is_empty(&self) -> bool;
}
