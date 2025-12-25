use starknet_api::hash::HashOutput;
use starknet_patricia::patricia_merkle_tree::filled_tree::node::FilledNode;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::Leaf;
use starknet_patricia::patricia_merkle_tree::traversal::SubTreeTrait;
use starknet_patricia_storage::db_object::{DBObject, HasStaticPrefix};

use crate::db::index_db::leaves::TrieType;

/// Specifies the trie db layout.
pub trait NodeLayout<'a, L: Leaf> {
    /// Additional data that a node stores about its children.
    ///
    /// NodeData is empty for index layout, and HashOutput for facts layout. The From<HashOutput>
    /// constraint is used in two cases where we're generic in the layout but hold a concrete
    /// HashOutput:
    /// 1. When [starknet_patricia::patricia_merkle_tree::traversal::SubTreeTrait::create] is called
    ///    in the beginning of the original skeleton tree creation.
    /// 2. When Layout::get_db_object is called in the serialization of a FilledTree.
    type NodeData: Clone + From<HashOutput>;

    /// The context needed to deserialize the node from a raw
    /// [starknet_patricia_storage::storage_trait::DbValue].
    type DeserializationContext;

    /// The storage representation of the node.
    type NodeDbObject: DBObject<DeserializeContext = Self::DeserializationContext>
        + Into<FilledNode<L, Self::NodeData>>;

    /// The type of the subtree that is used to traverse the trie.
    type SubTree: SubTreeTrait<
            'a,
            NodeData = Self::NodeData,
            NodeDeserializeContext = Self::DeserializationContext,
        >;

    /// Generates the key context for the given trie type. Used for reading nodes of a specific
    /// tree (contracts, classes, or storage), to construct a skeleton tree.
    fn generate_key_context(trie_type: TrieType) -> <L as HasStaticPrefix>::KeyContext;
}
