use starknet_patricia::patricia_merkle_tree::filled_tree::node::FilledNode;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::Leaf;
use starknet_patricia::patricia_merkle_tree::traversal::SubTreeTrait;
use starknet_patricia_storage::db_object::DBObject;

/// A trait that specifies the trie db layout.
///
/// The layout determines:
/// [NodeData]: additional data that a node stores about its children.
/// [DeserializationContext]: the context needed to deserialize the node from a raw [DbValue].
/// [SubTree]: the type of the subtree that is used to traverse the trie.
pub trait NodeLayout<'a, L: Leaf>
where
    FilledNode<L, Self::NodeData>: DBObject<DeserializeContext = Self::DeserializationContext>,
{
    type NodeData: Copy;
    type DeserializationContext;
    type SubTree: SubTreeTrait<
            'a,
            NodeData = Self::NodeData,
            NodeDeserializeContext = Self::DeserializationContext,
        >;
}
