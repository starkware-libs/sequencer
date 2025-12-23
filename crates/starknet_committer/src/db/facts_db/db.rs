use std::collections::HashMap;

use async_trait::async_trait;
use starknet_api::core::ContractAddress;
use starknet_api::hash::HashOutput;
use starknet_patricia::db_layout::{NodeLayout, TrieType};
use starknet_patricia::patricia_merkle_tree::filled_tree::node::{FactDbFilledNode, FilledNode};
use starknet_patricia::patricia_merkle_tree::filled_tree::node_serde::FactNodeDeserializationContext;
use starknet_patricia::patricia_merkle_tree::filled_tree::tree::FilledTree;
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::NodeData;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::{Leaf, LeafModifications};
use starknet_patricia::patricia_merkle_tree::traversal::SubTreeTrait;
use starknet_patricia::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices};
use starknet_patricia_storage::db_object::{EmptyKeyContext, HasStaticPrefix};
use starknet_patricia_storage::errors::SerializationResult;
use starknet_patricia_storage::map_storage::MapStorage;
use starknet_patricia_storage::storage_trait::{DbHashMap, Storage};

use crate::block_committer::input::{FactsDbInitialRead, ReaderConfig, StarknetStorageValue};
use crate::db::facts_db::types::FactsSubTree;
use crate::db::forest_trait::{ForestReader, ForestWriter};
use crate::db::trie_traversal::{create_classes_trie, create_contracts_trie, create_storage_tries};
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

impl<'a, L: Leaf> NodeLayout<'a, L> for FactsNodeLayout
where
    L: HasStaticPrefix<KeyContext = EmptyKeyContext>,
{
    type NodeData = HashOutput;

    type NodeDbObject = FactDbFilledNode<L>;

    type DeserializationContext = FactNodeDeserializationContext;

    type SubTree = FactsSubTree<'a>;

    fn create_subtree(
        sorted_leaf_indices: SortedLeafIndices<'a>,
        root_index: NodeIndex,
        root_hash: HashOutput,
    ) -> Self::SubTree {
        FactsSubTree::create(sorted_leaf_indices, root_index, root_hash)
    }

    fn generate_key_context(_trie_type: TrieType) -> <L as HasStaticPrefix>::KeyContext {
        EmptyKeyContext
    }

    fn get_filled_node(node_db_object: Self::NodeDbObject) -> FilledNode<L, Self::NodeData> {
        node_db_object
    }

    fn get_db_object(
        hash: HashOutput,
        filled_node_data: NodeData<L, HashOutput>,
    ) -> Self::NodeDbObject {
        FilledNode { hash, data: filled_node_data }
    }

    fn get_node_suffix(_index: NodeIndex, node_db_object: &Self::NodeDbObject) -> Vec<u8> {
        node_db_object.hash.0.to_bytes_be().to_vec()
    }
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

#[async_trait]
impl<S: Storage> ForestReader<FactsDbInitialRead> for FactsDb<S> {
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
            create_contracts_trie::<ContractState, FactsNodeLayout>(
                &mut self.storage,
                context.0.contracts_trie_root_hash,
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
impl<S: Storage> ForestWriter for FactsDb<S> {
    fn serialize_forest(filled_forest: &FilledForest) -> SerializationResult<DbHashMap> {
        let mut serialized_forest = DbHashMap::new();

        // Storage tries.
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
