use starknet_api::hash::HashOutput;
use starknet_patricia::patricia_merkle_tree::filled_tree::node::FilledNode;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::Leaf;
use starknet_patricia::patricia_merkle_tree::traversal::SubTreeTrait;
use starknet_patricia::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices};
use starknet_patricia_storage::db_object::HasStaticPrefix;
use starknet_patricia_storage::errors::DeserializationError;
use starknet_patricia_storage::storage_trait::DbValue;

use crate::db::index_db::leaves::TrieType;

pub trait NodeLayout<'a, L: Leaf> {
    type ChildData: Copy;
    type DeserializationContext;
    type SubTree: SubTreeTrait<'a, ChildData = Self::ChildData, NodeContext = Self::DeserializationContext>;
    fn deserialize_node(
        value: &DbValue,
        deserialize_context: &Self::DeserializationContext,
    ) -> Result<FilledNode<L, Self::ChildData>, DeserializationError>;
    fn create_subtree(
        sorted_leaf_indices: SortedLeafIndices<'a>,
        root_index: NodeIndex,
        root_hash: HashOutput,
    ) -> Self::SubTree;
    fn generate_key_context(trie_type: TrieType) -> <L as HasStaticPrefix>::KeyContext;
}
