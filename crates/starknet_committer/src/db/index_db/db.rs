use std::collections::HashMap;

use async_trait::async_trait;
use starknet_api::core::ContractAddress;
use starknet_patricia::db_layout::{NodeLayout, TrieType};
use starknet_patricia::patricia_merkle_tree::filled_tree::tree::FilledTree;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::{Leaf, LeafModifications};
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;
use starknet_patricia::patricia_merkle_tree::updated_skeleton_tree::hash_function::TreeHashFunction;
use starknet_patricia_storage::db_object::{EmptyKeyContext, HasStaticPrefix};
use starknet_patricia_storage::errors::SerializationResult;
use starknet_patricia_storage::storage_trait::{DbHashMap, Storage};

use crate::block_committer::input::{ReaderConfig, StarknetStorageValue};
use crate::db::facts_db::types::FactsDbInitialRead;
use crate::db::forest_trait::{ForestReader, ForestWriter};
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
use crate::db::trie_traversal::{create_classes_trie, create_contracts_trie, create_storage_tries};
use crate::forest::filled_forest::FilledForest;
use crate::forest::forest_errors::ForestResult;
use crate::forest::original_skeleton_forest::{ForestSortedIndices, OriginalSkeletonForest};
use crate::hash_function::hash::TreeHashFunctionImpl;
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use crate::patricia_merkle_tree::types::CompiledClassHash;

pub struct IndexDb<S: Storage> {
    pub storage: S,
}

impl<S: Storage> IndexDb<S> {
    pub fn new(storage: S) -> Self {
        Self { storage }
    }
}

pub struct IndexNodeLayout {}

impl<'a, L: Leaf> NodeLayout<'a, L> for IndexNodeLayout
where
    L: HasStaticPrefix<KeyContext = TrieType>,
    TreeHashFunctionImpl: TreeHashFunction<L>,
{
    type NodeData = EmptyNodeData;
    type NodeDbObject = IndexFilledNode<L>;
    type DeserializationContext = IndexNodeContext;
    type SubTree = IndexLayoutSubTree<'a>;

    fn generate_key_context(trie_type: TrieType) -> <L as HasStaticPrefix>::KeyContext {
        trie_type
    }
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
        let (contracts_trie, original_contracts_trie_leaves) =
            create_contracts_trie::<IndexLayoutContractState, IndexNodeLayout>(
                &mut self.storage,
                context.0.contracts_trie_root_hash,
                forest_sorted_indices.contracts_trie_sorted_indices,
            )
            .await?;
        let storage_tries =
            create_storage_tries::<IndexLayoutStarknetStorageValue, IndexNodeLayout>(
                &mut self.storage,
                storage_updates,
                &original_contracts_trie_leaves,
                &config,
                &forest_sorted_indices.storage_tries_sorted_indices,
            )
            .await?;
        let classes_trie = create_classes_trie::<IndexLayoutCompiledClassHash, IndexNodeLayout>(
            &mut self.storage,
            classes_updates,
            context.0.classes_trie_root_hash,
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

#[async_trait]
impl<S: Storage> ForestWriter for IndexDb<S> {
    fn serialize_forest(filled_forest: &FilledForest) -> SerializationResult<DbHashMap> {
        let mut serialized_forest = DbHashMap::new();

        // TODO(Ariel): use a different key context when FilledForest is generic over leaf types.
        for tree in filled_forest.storage_tries.values() {
            serialized_forest.extend(tree.serialize(&EmptyKeyContext)?);
        }

        // Contracts and classes tries.
        serialized_forest.extend(filled_forest.contracts_trie.serialize(&EmptyKeyContext)?);
        serialized_forest.extend(filled_forest.classes_trie.serialize(&EmptyKeyContext)?);

        Ok(serialized_forest)
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
