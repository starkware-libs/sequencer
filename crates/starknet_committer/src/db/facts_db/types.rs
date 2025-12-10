use starknet_api::hash::HashOutput;
use starknet_patricia::patricia_merkle_tree::filled_tree::node_serde::PatriciaPrefix;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::Leaf;
use starknet_patricia::patricia_merkle_tree::traversal::SubTreeTrait;
use starknet_patricia::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices};
use starknet_patricia_storage::storage_trait::DbKeyPrefix;

#[derive(Debug, PartialEq)]
pub struct FactsSubTree<'a> {
    pub sorted_leaf_indices: SortedLeafIndices<'a>,
    pub root_index: NodeIndex,
    pub root_hash: HashOutput,
}

impl<'a> SubTreeTrait<'a> for FactsSubTree<'a> {
    type ChildData = HashOutput;

    fn create_child(
        sorted_leaf_indices: SortedLeafIndices<'a>,
        root_index: NodeIndex,
        child_data: Self::ChildData,
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

    fn get_root_prefix<L: Leaf>(&self) -> DbKeyPrefix {
        if self.is_leaf() {
            PatriciaPrefix::Leaf(L::get_static_prefix()).into()
        } else {
            PatriciaPrefix::InnerNode.into()
        }
    }
}
