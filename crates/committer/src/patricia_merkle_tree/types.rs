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
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    derive_more::Add,
    derive_more::Mul,
    derive_more::Sub,
    PartialOrd,
    Ord,
)]
pub(crate) struct NodeIndex(pub Felt);

#[allow(dead_code)]
impl NodeIndex {
    pub(crate) fn root_index() -> NodeIndex {
        NodeIndex(Felt::ONE)
    }

    pub(crate) fn compute_bottom_index(
        index: NodeIndex,
        path_to_bottom: &PathToBottom,
    ) -> NodeIndex {
        let PathToBottom { path, length } = path_to_bottom;
        index.times_two_to_the_power(length.0) + NodeIndex(path.0)
    }

    pub(crate) fn times_two_to_the_power(&self, power: u8) -> Self {
        NodeIndex(self.0.times_two_to_the_power(power))
    }
}

impl From<u128> for NodeIndex {
    fn from(value: u128) -> Self {
        Self(Felt::from(value))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct EdgePath(pub Felt);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct EdgePathLength(pub u8);

#[allow(dead_code)]
#[derive(Debug, Eq, PartialEq, derive_more::Sub)]
pub(crate) struct TreeHeight(pub u8);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct PathToBottom {
    pub path: EdgePath,
    pub length: EdgePathLength,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct EdgeData {
    pub(crate) bottom_hash: HashOutput,
    pub(crate) path_to_bottom: PathToBottom,
}

pub(crate) trait LeafDataTrait {
    /// Returns true if leaf is empty.
    fn is_empty(&self) -> bool;
}

impl PathToBottom {
    pub(crate) fn bottom_index(&self, root_index: NodeIndex) -> NodeIndex {
        NodeIndex::compute_bottom_index(root_index, self)
    }
}
