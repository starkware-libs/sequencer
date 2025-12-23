use starknet_api::core::ContractAddress;
use starknet_api::hash::HashOutput;
use starknet_patricia_storage::db_object::{DBObject, HasStaticPrefix};

use crate::patricia_merkle_tree::filled_tree::node::FilledNode;
use crate::patricia_merkle_tree::node_data::inner_node::NodeData;
use crate::patricia_merkle_tree::node_data::leaf::Leaf;
use crate::patricia_merkle_tree::traversal::SubTreeTrait;
use crate::patricia_merkle_tree::types::NodeIndex;

// TODO(Ariel): Delete this enum and use `CommitmentType` instead.
#[derive(Debug, PartialEq)]
pub enum TrieType {
    ContractsTrie,
    ClassesTrie,
    StorageTrie(ContractAddress),
}

/// Specifies the trie db layout.
pub trait NodeLayout<'a, L: Leaf> {
    /// Additional data that a node stores about its children.
    type NodeData: Clone + From<HashOutput>;

    /// The context needed to deserialize the node from a raw
    /// [starknet_patricia_storage::storage_trait::DbValue].
    type DeserializationContext;

    /// The storage representation of the node.
    type NodeDbObject: DBObject<
            DeserializeContext = Self::DeserializationContext,
            KeyContext = <L as HasStaticPrefix>::KeyContext,
        > + Into<FilledNode<L, Self::NodeData>>;

    /// The type of the subtree that is used to traverse the trie.
    type SubTree: SubTreeTrait<
            'a,
            NodeData = Self::NodeData,
            NodeDeserializeContext = Self::DeserializationContext,
        >;

    /// Generates the key context for the given trie type. Used for reading nodes of a specific
    /// tree (contracts, classes, or storage), to construct a skeleton tree.
    fn generate_key_context(trie_type: TrieType) -> <L as HasStaticPrefix>::KeyContext;

    /// Converts `FilledTree` nodes to db objects.
    ///
    /// During the construction of a `FilledTree` we computee the hashes and carry `FilledNode<L,
    /// HashOutput>`, hence, the `FilledNode` type  is not necessarily what we want to store.
    fn get_db_object(
        hash: HashOutput,
        filled_node_data: NodeData<L, HashOutput>,
    ) -> Self::NodeDbObject;

    /// Returns the db key suffix of the node db object.
    fn get_node_suffix(index: NodeIndex, node_db_object: &Self::NodeDbObject) -> Vec<u8>;
}
