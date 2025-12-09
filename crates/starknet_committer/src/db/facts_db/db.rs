use std::collections::HashMap;

use starknet_api::core::ContractAddress;
use starknet_api::hash::HashOutput;
use starknet_patricia::patricia_merkle_tree::filled_tree::node_serde::NodeContext;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::{Leaf, LeafModifications};
use starknet_patricia::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices};
use starknet_patricia_storage::db_object::{DBObject, HasStaticPrefix};
use starknet_patricia_storage::errors::DeserializationError;
use starknet_patricia_storage::map_storage::MapStorage;
use starknet_patricia_storage::storage_trait::{DbValue, Storage};

use crate::block_committer::input::{ConfigImpl, StarknetStorageValue};
use crate::db::db_layout::NodeLayout;
use crate::db::facts_db::types::{FactDbFilledNode, FactsSubTree};
use crate::db::forest_trait::{ForestReader, ForestWriter};
use crate::db::index_db::leaves::TrieType;
use crate::db::trie_traversal::{create_classes_trie, create_contracts_trie, create_storage_tries};
use crate::forest::filled_forest::FilledForest;
use crate::forest::forest_errors::ForestResult;
use crate::forest::original_skeleton_forest::{ForestSortedIndices, OriginalSkeletonForest};
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use crate::patricia_merkle_tree::types::CompiledClassHash;

pub struct FactsNodeLayout {}

impl<'a, L: Leaf> NodeLayout<'a, L> for FactsNodeLayout
where
    L: HasStaticPrefix<KeyContext = ()>,
{
    type ChildData = HashOutput;
    type DeserializationContext = NodeContext;
    type SubTree = FactsSubTree<'a>;
    fn create_subtree(
        sorted_leaf_indices: SortedLeafIndices<'a>,
        root_index: NodeIndex,
        root_hash: HashOutput,
    ) -> Self::SubTree {
        FactsSubTree { sorted_leaf_indices, root_index, root_hash }
    }
    fn deserialize_node(
        value: &DbValue,
        deserialize_context: &Self::DeserializationContext,
    ) -> Result<FactDbFilledNode<L>, DeserializationError> {
        DBObject::deserialize(value, deserialize_context)
    }
    fn generate_key_context(_trie_type: TrieType) -> <L as HasStaticPrefix>::KeyContext {}
}
pub struct FactsDb<S: Storage> {
    // TODO(Yoav): Define StorageStats trait and impl it here. Then, make the storage field
    // private.
    pub storage: S,
}

impl<S: Storage> FactsDb<S> {
    pub fn new(storage: S) -> Self {
        Self { storage }
    }
}

impl FactsDb<MapStorage> {
    pub fn consume_storage(self) -> MapStorage {
        self.storage
    }
}

impl<'a, S: Storage> ForestReader<'a> for FactsDb<S> {
    /// Creates an original skeleton forest that includes the storage tries of the modified
    /// contracts, the classes trie and the contracts trie. Additionally, returns the original
    /// contract states that are needed to compute the contract state tree.
    async fn read(
        &mut self,
        contracts_trie_root_hash: HashOutput,
        classes_trie_root_hash: HashOutput,
        storage_updates: &'a HashMap<ContractAddress, LeafModifications<StarknetStorageValue>>,
        classes_updates: &'a LeafModifications<CompiledClassHash>,
        forest_sorted_indices: &'a ForestSortedIndices<'a>,
        config: ConfigImpl,
    ) -> ForestResult<(OriginalSkeletonForest<'a>, HashMap<NodeIndex, ContractState>)> {
        let (contracts_trie, original_contracts_trie_leaves) =
            create_contracts_trie::<ContractState, FactsNodeLayout>(
                &mut self.storage,
                contracts_trie_root_hash,
                forest_sorted_indices.contracts_trie_sorted_indices,
            )
            .await?;
        let storage_tries = create_storage_tries::<StarknetStorageValue, FactsNodeLayout>(
            &mut self.storage,
            storage_updates,
            &original_contracts_trie_leaves,
            &config,
            &forest_sorted_indices.storage_tries_sorted_indices,
        )
        .await?;
        let classes_trie = create_classes_trie::<CompiledClassHash, FactsNodeLayout>(
            &mut self.storage,
            classes_updates,
            classes_trie_root_hash,
            &config,
            forest_sorted_indices.classes_trie_sorted_indices,
        )
        .await?;

        Ok((
            OriginalSkeletonForest { classes_trie, contracts_trie, storage_tries },
            original_contracts_trie_leaves,
        ))
    }
}

impl<S: Storage> ForestWriter for FactsDb<S> {
    async fn write(&mut self, filled_forest: &FilledForest) -> usize {
        filled_forest.write_to_storage(&mut self.storage).await
    }
}
