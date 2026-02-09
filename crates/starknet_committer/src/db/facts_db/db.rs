use std::collections::HashMap;

use async_trait::async_trait;
use starknet_api::core::ContractAddress;
use starknet_api::hash::{HashOutput, StateRoots};
use starknet_patricia::db_layout::{NodeLayout, NodeLayoutFor};
use starknet_patricia::patricia_merkle_tree::filled_tree::node::{FactDbFilledNode, FilledNode};
use starknet_patricia::patricia_merkle_tree::filled_tree::node_serde::FactNodeDeserializationContext;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::{Leaf, LeafModifications};
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;
use starknet_patricia_storage::db_object::{DBObject, HasStaticPrefix};
use starknet_patricia_storage::errors::SerializationResult;
use starknet_patricia_storage::map_storage::MapStorage;
use starknet_patricia_storage::storage_trait::{
    DbHashMap,
    DbKey,
    DbOperationMap,
    PatriciaStorageResult,
    Storage,
};

use crate::block_committer::input::{ReaderConfig, StarknetStorageValue};
use crate::db::db_layout::DbLayout;
use crate::db::facts_db::types::{FactsDbInitialRead, FactsSubTree};
use crate::db::forest_trait::{
    read_forest,
    serialize_forest,
    ForestReader,
    ForestWriter,
    StorageInitializer,
};
use crate::forest::filled_forest::FilledForest;
use crate::forest::forest_errors::ForestResult;
use crate::forest::original_skeleton_forest::{ForestSortedIndices, OriginalSkeletonForest};
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use crate::patricia_merkle_tree::types::CompiledClassHash;

/// Facts DB node layout.
///
/// In a facts DB, the storage keys are node hashes and the values are preimages. In particular,
/// each nodes holds its child node hashes. In this layout, only once the  parent is traversed we
/// have the db keys of its children.
pub struct FactsNodeLayout {}

impl<'a, L: Leaf> NodeLayout<'a, L> for FactsNodeLayout {
    type NodeData = HashOutput;

    type NodeDbObject = FactDbFilledNode<L>;

    type DeserializationContext = FactNodeDeserializationContext;

    type SubTree = FactsSubTree<'a>;

    fn get_db_object<LeafBase: Leaf + Into<L>>(
        _node_index: NodeIndex,
        key_context: &<L as HasStaticPrefix>::KeyContext,
        filled_node: FilledNode<LeafBase, HashOutput>,
    ) -> (DbKey, Self::NodeDbObject) {
        let db_filled_node = Self::convert_node_data_and_leaf(filled_node);

        let suffix = &db_filled_node.hash.0.to_bytes_be();
        let key = db_filled_node.get_db_key(key_context, suffix);

        (key, db_filled_node)
    }
}

impl NodeLayoutFor<StarknetStorageValue> for FactsNodeLayout {
    type DbLeaf = StarknetStorageValue;
}

impl NodeLayoutFor<ContractState> for FactsNodeLayout {
    type DbLeaf = ContractState;
}

impl NodeLayoutFor<CompiledClassHash> for FactsNodeLayout {
    type DbLeaf = CompiledClassHash;
}

impl DbLayout for FactsNodeLayout {
    type ContractStateDbLeaf = ContractState;
    type CompiledClassHashDbLeaf = CompiledClassHash;
    type StarknetStorageValueDbLeaf = StarknetStorageValue;
    type NodeLayout = FactsNodeLayout;
}

pub struct FactsDb<S: Storage> {
    // TODO(Yoav): Define StorageStats trait and impl it here. Then, make the storage field
    // private.
    pub storage: S,
}

impl<S: Storage> StorageInitializer for FactsDb<S> {
    type Storage = S;
    fn new(storage: Self::Storage) -> Self {
        Self { storage }
    }
}

impl FactsDb<MapStorage> {
    pub fn consume_storage(self) -> MapStorage {
        self.storage
    }
}

#[async_trait]
impl<S: Storage> ForestReader for FactsDb<S> {
    type InitialReadContext = FactsDbInitialRead;

    /// Creates an original skeleton forest that includes the storage tries of the modified
    /// contracts, the classes trie and the contracts trie. Additionally, returns the original
    /// contract states that are needed to compute the contract state tree.
    async fn read<'a>(
        &mut self,
        roots: StateRoots,
        storage_updates: &'a HashMap<ContractAddress, LeafModifications<StarknetStorageValue>>,
        classes_updates: &'a LeafModifications<CompiledClassHash>,
        forest_sorted_indices: &'a ForestSortedIndices<'a>,
        config: ReaderConfig,
    ) -> ForestResult<(OriginalSkeletonForest<'a>, HashMap<NodeIndex, ContractState>)> {
        read_forest::<S, FactsNodeLayout>(
            &mut self.storage,
            roots,
            storage_updates,
            classes_updates,
            forest_sorted_indices,
            config,
        )
        .await
    }

    async fn read_roots(
        &mut self,
        initial_read_context: Self::InitialReadContext,
    ) -> PatriciaStorageResult<StateRoots> {
        Ok(initial_read_context.0)
    }
}

#[async_trait]
impl<S: Storage> ForestWriter for FactsDb<S> {
    fn serialize_forest(filled_forest: &FilledForest) -> SerializationResult<DbHashMap> {
        serialize_forest::<FactsNodeLayout>(filled_forest)
    }

    async fn write_updates(&mut self, updates: DbOperationMap) -> usize {
        let n_updates = updates.len();
        self.storage
            .multi_set_and_delete(updates)
            .await
            .unwrap_or_else(|_| panic!("Write of {n_updates} new updates to storage failed"));
        n_updates
    }
}
