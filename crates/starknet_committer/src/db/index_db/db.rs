use std::collections::HashMap;

use async_trait::async_trait;
use starknet_api::core::ContractAddress;
use starknet_api::hash::HashOutput;
use starknet_patricia::db_layout::{NodeLayout, NodeLayoutFor};
use starknet_patricia::patricia_merkle_tree::filled_tree::node::FilledNode;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::{Leaf, LeafModifications};
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;
use starknet_patricia::patricia_merkle_tree::updated_skeleton_tree::hash_function::TreeHashFunction;
use starknet_patricia_storage::db_object::{DBObject, HasStaticPrefix};
use starknet_patricia_storage::errors::SerializationResult;
use starknet_patricia_storage::storage_trait::{DbHashMap, DbKey, Storage};

use crate::block_committer::input::{ReaderConfig, StarknetStorageValue};
use crate::db::facts_db::types::FactsDbInitialRead;
use crate::db::forest_trait::{read_forest, serialize_forest, ForestReader, ForestWriter};
use crate::db::index_db::leaves::{
    IndexLayoutCompiledClassHash,
    IndexLayoutContractState,
    IndexLayoutStarknetStorageValue,
};
use crate::db::index_db::types::{
    EmptyNodeData,
    IndexFilledNode,
    IndexLayoutSubTree,
    IndexNodeContext,
};
use crate::forest::filled_forest::FilledForest;
use crate::forest::forest_errors::ForestResult;
use crate::forest::original_skeleton_forest::{ForestSortedIndices, OriginalSkeletonForest};
use crate::hash_function::hash::TreeHashFunctionImpl;
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use crate::patricia_merkle_tree::types::CompiledClassHash;

pub struct IndexDb<S: Storage> {
    storage: S,
}

impl<S: Storage> IndexDb<S> {
    pub fn new(storage: S) -> Self {
        Self { storage }
    }
}

pub struct IndexNodeLayout {}

impl<'a, L> NodeLayout<'a, L> for IndexNodeLayout
where
    L: Leaf,
    TreeHashFunctionImpl: TreeHashFunction<L>,
{
    type NodeData = EmptyNodeData;
    type NodeDbObject = IndexFilledNode<L>;
    type DeserializationContext = IndexNodeContext;
    type SubTree = IndexLayoutSubTree<'a>;

    fn get_db_object<LeafBase: Leaf + Into<L>>(
        node_index: NodeIndex,
        key_context: &<L as HasStaticPrefix>::KeyContext,
        filled_node: FilledNode<LeafBase, HashOutput>,
    ) -> (DbKey, Self::NodeDbObject) {
        let filled_node = Self::convert_node_data_and_leaf(filled_node);

        let db_filled_node = IndexFilledNode(filled_node);

        let key = db_filled_node.get_db_key(key_context, &node_index.0.to_be_bytes());

        (key, db_filled_node)
    }
}

impl NodeLayoutFor<StarknetStorageValue> for IndexNodeLayout {
    type DbLeaf = IndexLayoutStarknetStorageValue;
}

impl NodeLayoutFor<ContractState> for IndexNodeLayout {
    type DbLeaf = IndexLayoutContractState;
}

impl NodeLayoutFor<CompiledClassHash> for IndexNodeLayout {
    type DbLeaf = IndexLayoutCompiledClassHash;
}

// TODO(Ariel): define an IndexDbInitialRead empty type, and check whether each tree is empty inside
// create_xxx_trie.
#[async_trait]
impl<S: Storage> ForestReader<FactsDbInitialRead> for IndexDb<S> {
    /// Creates an original skeleton forest that includes the storage tries of the modified
    /// contracts, the classes trie and the contracts trie. Additionally, returns the original
    /// contract states that are needed to compute the contract state tree.
    async fn read<'a>(
        &mut self,
        context: FactsDbInitialRead,
        storage_updates: &'a HashMap<ContractAddress, LeafModifications<StarknetStorageValue>>,
        classes_updates: &'a LeafModifications<CompiledClassHash>,
        forest_sorted_indices: &'a ForestSortedIndices<'a>,
        config: ReaderConfig,
    ) -> ForestResult<(OriginalSkeletonForest<'a>, HashMap<NodeIndex, ContractState>)> {
        read_forest::<S, IndexNodeLayout>(
            &mut self.storage,
            context,
            storage_updates,
            classes_updates,
            forest_sorted_indices,
            config,
        )
        .await
    }
}

#[async_trait]
impl<S: Storage> ForestWriter for IndexDb<S> {
    fn serialize_forest(filled_forest: &FilledForest) -> SerializationResult<DbHashMap> {
        serialize_forest::<IndexNodeLayout>(filled_forest)
    }

    async fn write_updates(&mut self, updates: DbHashMap) -> usize {
        let n_updates = updates.len();
        self.storage
            .mset(updates)
            .await
            .unwrap_or_else(|_| panic!("Write of {n_updates} new updates to storage failed"));
        n_updates
    }
}
