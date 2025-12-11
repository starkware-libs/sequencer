use starknet_patricia::patricia_merkle_tree::filled_tree::node::FilledNode;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::Leaf;
use starknet_patricia_storage::errors::DeserializationError;
use starknet_patricia_storage::storage_trait::DbValue;

pub trait NodeLayout<L: Leaf> {
    type ChildData: Copy;
    type DeserializationContext;
    fn deserialize_node(
        value: &DbValue,
        deserialize_context: &Self::DeserializationContext,
    ) -> Result<FilledNode<L, Self::ChildData>, DeserializationError>;
}
