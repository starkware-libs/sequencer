use starknet_patricia::patricia_merkle_tree::filled_tree::node::FilledNode;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::Leaf;
use starknet_patricia::patricia_merkle_tree::traversal::{SubTreeTrait, UnmodifiedChildTraversal};
use starknet_patricia::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices};
use starknet_patricia_storage::db_object::HasStaticPrefix;
use starknet_patricia_storage::storage_trait::DbKeyPrefix;

pub struct IndexFilledNode<L: Leaf>(pub FilledNode<L, ()>);

pub struct IndexNodeContext {
    pub is_leaf: bool,
}

pub struct IndexLayoutSubTree<'a> {
    pub sorted_leaf_indices: SortedLeafIndices<'a>,
    pub root_index: NodeIndex,
}

impl<'a> SubTreeTrait<'a> for IndexLayoutSubTree<'a> {
    type NodeData = ();
    type NodeDeserializeContext = IndexNodeContext;

    fn create(
        sorted_leaf_indices: SortedLeafIndices<'a>,
        root_index: NodeIndex,
        _child_data: Self::NodeData,
    ) -> Self {
        Self { sorted_leaf_indices, root_index }
    }

    fn get_root_index(&self) -> NodeIndex {
        self.root_index
    }

    fn get_sorted_leaf_indices(&self) -> &SortedLeafIndices<'a> {
        &self.sorted_leaf_indices
    }

    fn should_traverse_unmodified_child(_data: Self::NodeData) -> UnmodifiedChildTraversal {
        UnmodifiedChildTraversal::Traverse
    }

    fn get_root_context(&self) -> Self::NodeDeserializeContext {
        Self::NodeDeserializeContext { is_leaf: self.is_leaf() }
    }

    fn get_root_prefix<L: Leaf>(
        &self,
        _key_context: &<L as HasStaticPrefix>::KeyContext,
    ) -> DbKeyPrefix {
        L::get_static_prefix(_key_context)
    }

    fn get_root_suffix(&self) -> Vec<u8> {
        self.root_index.0.to_be_bytes().to_vec()
    }
}
