use crate::{
    hash::types::{HashOutput, PedersenHashFunction},
    patricia_merkle_tree::filled_node::{LeafData, NodeData},
};

use super::TreeHashFunction;

pub(crate) struct MockTreeHashFunctionImpl;

/// Mock implementation of TreeHashFunction for testing purposes.
impl TreeHashFunction<LeafData, PedersenHashFunction> for MockTreeHashFunctionImpl {
    fn compute_node_hash(node_data: NodeData<LeafData>) -> HashOutput {
        match node_data {
            NodeData::Binary(_) => todo!(),
            NodeData::Edge(_) => todo!(),
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
