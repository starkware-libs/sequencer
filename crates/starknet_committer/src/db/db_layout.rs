use starknet_patricia::patricia_merkle_tree::filled_tree::node::FilledNode;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::Leaf;
use starknet_patricia::patricia_merkle_tree::traversal::SubTreeTrait;
use starknet_patricia_storage::db_object::DBObject;

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

    /// The type of the subtree that is used to traverse the trie.
    type SubTree: SubTreeTrait<
            'a,
            NodeData = Self::NodeData,
            NodeDeserializeContext = Self::DeserializationContext,
        >;
}
