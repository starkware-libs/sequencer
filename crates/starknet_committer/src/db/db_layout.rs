use starknet_api::hash::HashOutput;
use starknet_patricia::patricia_merkle_tree::filled_tree::node::FilledNode;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::Leaf;
use starknet_patricia::patricia_merkle_tree::traversal::SubTreeTrait;
use starknet_patricia::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices};
use starknet_patricia_storage::db_object::{DBObject, HasStaticPrefix};

use crate::db::index_db::leaves::TrieType;

/// Specifies the trie db layout.
pub trait NodeLayout<'a, L: Leaf>
where
    FilledNode<L, Self::NodeData>: DBObject<DeserializeContext = Self::DeserializationContext>,
{
    /// Additional data that a node stores about its children.
    type NodeData: Clone;

    /// The context needed to deserialize the node from a raw
    /// [starknet_patricia_storage::storage_trait::DbValue].
    type DeserializationContext;

    /// The storage representation of the node.
    type NodeDbObject: DBObject<DeserializeContext = Self::DeserializationContext>;

    /// The type of the subtree that is used to traverse the trie.
    type SubTree: SubTreeTrait<
            'a,
            NodeData = Self::NodeData,
            NodeDeserializeContext = Self::DeserializationContext,
        >;

    /// Initializes a subtree for the beginning of the construction of the `OriginalSkeletonTree`.
    fn create_subtree(
        sorted_leaf_indices: SortedLeafIndices<'a>,
        root_index: NodeIndex,
        root_hash: HashOutput,
    ) -> Self::SubTree;

    /// Generates the key context for the given trie type. Used before the contruction of a specific
    /// skeleton tree (contracts, classes, or storage).
    fn generate_key_context(trie_type: TrieType) -> <L as HasStaticPrefix>::KeyContext;

    /// Converts a node db object to a filled node. Used during the trie traversal for the skeleton
    /// construction.
    fn get_filled_node(node_db_object: Self::NodeDbObject) -> FilledNode<L, Self::NodeData>;
}
