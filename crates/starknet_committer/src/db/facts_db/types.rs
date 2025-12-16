use starknet_api::hash::HashOutput;
use starknet_patricia::patricia_merkle_tree::filled_tree::node_serde::PatriciaPrefix;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::Leaf;
use starknet_patricia::patricia_merkle_tree::traversal::SubTreeTrait;
use starknet_patricia::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices};
use starknet_patricia_storage::db_object::HasStaticPrefix;
use starknet_patricia_storage::storage_trait::DbKeyPrefix;

#[derive(Debug, PartialEq)]
pub struct FactsSubTree<'a> {
    sorted_leaf_indices: SortedLeafIndices<'a>,
    pub root_index: NodeIndex,
    pub root_hash: HashOutput,
}

impl<'a> SubTreeTrait<'a> for FactsSubTree<'a> {
    type NodeData = HashOutput;

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

    fn should_traverse_unmodified_children() -> bool {
        false
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
}
