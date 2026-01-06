use starknet_api::hash::{HashOutput, StateRoots};
use starknet_patricia::patricia_merkle_tree::filled_tree::node_serde::{
    FactNodeDeserializationContext,
    PatriciaPrefix,
};
use starknet_patricia::patricia_merkle_tree::node_data::leaf::Leaf;
use starknet_patricia::patricia_merkle_tree::traversal::{SubTreeTrait, UnmodifiedChildTraversal};
use starknet_patricia::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices};
use starknet_patricia_storage::db_object::HasStaticPrefix;
use starknet_patricia_storage::storage_trait::DbKeyPrefix;

use crate::block_committer::input::InputContext;

#[derive(Debug, PartialEq)]
pub struct FactsSubTree<'a> {
    sorted_leaf_indices: SortedLeafIndices<'a>,
    pub root_index: NodeIndex,
    pub root_hash: HashOutput,
}

impl<'a> SubTreeTrait<'a> for FactsSubTree<'a> {
    type NodeData = HashOutput;
    type NodeDeserializeContext = FactNodeDeserializationContext;

    fn create(
        sorted_leaf_indices: SortedLeafIndices<'a>,
        root_index: NodeIndex,
        child_data: Self::NodeData,
    ) -> Self {
        Self { sorted_leaf_indices, root_index, root_hash: child_data }
    }

    fn get_root_index(&self) -> NodeIndex {
        self.root_index
    }

    fn get_sorted_leaf_indices(&self) -> &SortedLeafIndices<'a> {
        &self.sorted_leaf_indices
    }

    fn should_traverse_unmodified_child(data: Self::NodeData) -> UnmodifiedChildTraversal {
        UnmodifiedChildTraversal::Skip(data)
    }

    fn get_root_context(&self) -> Self::NodeDeserializeContext {
        Self::NodeDeserializeContext { is_leaf: self.is_leaf(), node_hash: self.root_hash }
    }

    fn get_root_prefix<L: Leaf>(
        &self,
        key_context: &<L as HasStaticPrefix>::KeyContext,
    ) -> DbKeyPrefix {
        if self.is_leaf() {
            PatriciaPrefix::Leaf(L::get_static_prefix(key_context)).into()
        } else {
            PatriciaPrefix::InnerNode.into()
        }
    }

    fn get_root_suffix(&self) -> Vec<u8> {
        self.root_hash.0.to_bytes_be().to_vec()
    }
}
/// Used for reading the roots in facts layout case.
#[derive(Debug, PartialEq)]
pub struct FactsDbInitialRead(pub StateRoots);

impl InputContext for FactsDbInitialRead {}
