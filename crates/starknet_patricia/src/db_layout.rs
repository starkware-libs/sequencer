use starknet_api::hash::HashOutput;
use starknet_patricia_storage::db_object::{DBObject, HasStaticPrefix};
use starknet_patricia_storage::storage_trait::DbKey;

use crate::patricia_merkle_tree::filled_tree::node::FilledNode;
use crate::patricia_merkle_tree::node_data::inner_node::{BinaryData, EdgeData, NodeData};
use crate::patricia_merkle_tree::node_data::leaf::Leaf;
use crate::patricia_merkle_tree::traversal::SubTreeTrait;
use crate::patricia_merkle_tree::types::NodeIndex;

/// Specifies the trie db layout.
pub trait NodeLayout<'a, L: Leaf + std::fmt::Debug> {
    /// Additional data that a node stores about its children.
    ///
    /// NodeData is empty for index layout, and HashOutput for facts layout. The `From<HashOutput>`
    /// constraint is used in two cases where we're generic in the layout but hold a concrete
    /// HashOutput:
    /// 1. When [crate::patricia_merkle_tree::traversal::SubTreeTrait::create] is called in the
    ///    beginning of the original skeleton tree creation.
    /// 2. When Layout::get_db_object is called in the serialization of a FilledTree.
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

    /// Converts `FilledTree` nodes to db objects.
    ///
    /// During the construction of a `FilledTree` we compute the hashes and carry `FilledNode<L,
    /// HashOutput>`, hence, the `FilledNode` type is not necessarily what we want to store.
    fn get_db_object<LeafBase: Leaf + Into<L>>(
        node_index: NodeIndex,
        key_context: &<L as HasStaticPrefix>::KeyContext,
        filled_node: FilledNode<LeafBase, HashOutput>,
    ) -> (DbKey, Self::NodeDbObject);

    /// A utility function to convert a `FilledNode<LeafBase, HashOutput>` to a `FilledNode<L,
    /// Self::NodeData>`. Used during the serialization of a `FilledTree`.
    ///
    /// LeafBase is one of StarknetStorageValue, CompiledClassHash, or ContractState, while L can be
    /// layout-dependent wrappers.
    fn convert_node_data_and_leaf<LeafBase: Leaf + Into<L>>(
        filled_node: FilledNode<LeafBase, HashOutput>,
    ) -> FilledNode<L, Self::NodeData> {
        let node_data: NodeData<L, Self::NodeData> = match filled_node.data {
            NodeData::Binary(binary_data) => NodeData::Binary(BinaryData {
                left_data: binary_data.left_data.into(),
                right_data: binary_data.right_data.into(),
            }),
            NodeData::Edge(edge_data) => NodeData::Edge(EdgeData {
                bottom_data: edge_data.bottom_data.into(),
                path_to_bottom: edge_data.path_to_bottom,
            }),
            NodeData::Leaf(leaf) => NodeData::Leaf(leaf.into()),
        };

        FilledNode { hash: filled_node.hash, data: node_data }
    }
}

/// A layout trait for one of the base leaf types: StarknetStorageValue, ContractState, or
/// CompiledClassHash. Use `NodeLayout<DbLeaf>` when you only deal with the DB representation, use
/// `NodeLayoutFor<BaseLeaf>` when you need to refer to both the logical type and the DB type.
///
/// We require that the BaseLeaf type and its DB representation `NodeLayout::DbLeaf` are isomorphic.
/// The `From<BaseLeaf>` conversions are needed to convert incoming modifications, and the
/// `Into<BaseLeaf>` conversions are used in the `ContractState` case where we need to return the
/// leaves. For more details see `create_contracts_trie`, `create_classes_trie`, and
/// `create_storage_tries` in `starknet_committer`.
pub trait NodeLayoutFor<BaseLeaf>: for<'a> NodeLayout<'a, Self::DbLeaf> {
    /// This layout's DB representation of BaseLeaf.
    type DbLeaf: Leaf + From<BaseLeaf> + Into<BaseLeaf>;
}
