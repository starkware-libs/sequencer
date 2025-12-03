use std::collections::HashMap;

use starknet_api::core::ContractAddress;
use starknet_api::hash::HashOutput;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::{Leaf, LeafModifications};
use starknet_patricia::patricia_merkle_tree::original_skeleton_tree::config::OriginalSkeletonTreeConfig;
use starknet_patricia::patricia_merkle_tree::original_skeleton_tree::node::OriginalSkeletonNode;
use starknet_patricia::patricia_merkle_tree::original_skeleton_tree::tree::{
    OriginalSkeletonTreeImpl,
    OriginalSkeletonTreeResult,
};
use starknet_patricia::patricia_merkle_tree::traversal::{SubTree, TraversalError};
use starknet_patricia::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices};
use starknet_patricia::patricia_merkle_tree::updated_skeleton_tree::hash_function::TreeHashFunction;
use starknet_patricia_storage::errors::StorageError;
use starknet_patricia_storage::storage_trait::{DbKey, Storage};
use tracing::warn;

use crate::block_committer::input::{
    contract_address_into_node_index,
    Config,
    StarknetStorageValue,
};
use crate::db::db_utils::{
    handle_empty_subtree,
    log_trivial_modification,
    log_warning_for_empty_leaves,
};
use crate::db::forest_readers::TrieReader;
use crate::db::forest_trait::ForestWriter;
use crate::db::index_db::db_keys::{db_key_from_node_index_and_context, KeyContext, TrieType};
use crate::db::index_db::db_types::{IndexDbFilledNode, IndexLayoutLeaf, IndexNodeData};
use crate::forest::filled_forest::FilledForest;
use crate::forest::forest_errors::{ForestError, ForestResult};
use crate::hash_function::hash::TreeHashFunctionImpl;
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use crate::patricia_merkle_tree::tree::{
    OriginalSkeletonClassesTrieConfig,
    OriginalSkeletonContractsTrieConfig,
    OriginalSkeletonStorageTrieConfig,
};
use crate::patricia_merkle_tree::types::CompiledClassHash;

pub struct IndexDb<S: Storage> {
    pub storage: S,
}

impl<S: Storage> IndexDb<S> {
    pub fn new(storage: S) -> Self {
        Self { storage }
    }
}

/// Fetches the Patricia witnesses, required to build the original skeleton tree from storage.
/// Given a list of subtrees, traverses towards their leaves and fetches all non-empty,
/// unmodified nodes. If `compare_modified_leaves` is set, function logs out a warning when
/// encountering a trivial modification. Fills the previous leaf values if it is not none.
async fn fetch_nodes<'a, L: Leaf + IndexLayoutLeaf, Hasher: TreeHashFunction<L>>(
    skeleton_tree: &mut OriginalSkeletonTreeImpl<'a>,
    subtrees: Vec<SubTree<'a>>,
    storage: &mut impl Storage,
    leaf_modifications: &LeafModifications<L>,
    config: &impl OriginalSkeletonTreeConfig<L>,
    mut previous_leaves: Option<&mut HashMap<NodeIndex, L>>,
    key_context: &KeyContext,
) -> OriginalSkeletonTreeResult<()> {
    let mut current_subtrees = subtrees;
    let mut next_subtrees = Vec::new();
    while !current_subtrees.is_empty() {
        let should_fetch_modified_leaves =
            config.compare_modified_leaves() || previous_leaves.is_some();
        let filled_roots =
            get_roots_from_storage::<L, Hasher>(&current_subtrees, storage, key_context).await?;
        for (filled_root, subtree) in filled_roots.into_iter().zip(current_subtrees.iter()) {
            match filled_root.data {
                // Binary node.
                IndexNodeData::Binary => {
                    if subtree.is_unmodified() {
                        skeleton_tree.nodes.insert(
                            subtree.root_index,
                            OriginalSkeletonNode::UnmodifiedSubTree(filled_root.hash),
                        );
                        continue;
                    }
                    skeleton_tree.nodes.insert(subtree.root_index, OriginalSkeletonNode::Binary);
                    let (left_subtree, right_subtree) = subtree.get_children_subtrees();
                    for child in [left_subtree, right_subtree] {
                        if !child.is_leaf() || child.is_unmodified() || should_fetch_modified_leaves
                        {
                            next_subtrees.push(child);
                        }
                    }
                }
                // Edge node.
                IndexNodeData::Edge(path_to_bottom) => {
                    skeleton_tree
                        .nodes
                        .insert(subtree.root_index, OriginalSkeletonNode::Edge(path_to_bottom));

                    // Parse bottom.
                    let (bottom_subtree, previously_empty_leaves_indices) =
                        subtree.get_bottom_subtree(&path_to_bottom);

                    if subtree.is_unmodified() {
                        next_subtrees.push(bottom_subtree);
                        continue;
                    }

                    if let Some(ref mut leaves) = previous_leaves {
                        leaves.extend(
                            previously_empty_leaves_indices
                                .iter()
                                .map(|idx| (**idx, L::default()))
                                .collect::<HashMap<NodeIndex, L>>(),
                        );
                    }
                    log_warning_for_empty_leaves(
                        &previously_empty_leaves_indices,
                        leaf_modifications,
                        config,
                    )?;

                    if !bottom_subtree.is_leaf()
                        || bottom_subtree.is_unmodified()
                        || should_fetch_modified_leaves
                    {
                        next_subtrees.push(bottom_subtree);
                    }
                }
                // Leaf node.
                IndexNodeData::Leaf(previous_leaf) => {
                    if subtree.is_unmodified() {
                        skeleton_tree.nodes.insert(
                            subtree.root_index,
                            OriginalSkeletonNode::UnmodifiedSubTree(filled_root.hash),
                        );
                    } else {
                        // Modified leaf.
                        if config.compare_modified_leaves()
                            && L::compare(leaf_modifications, &subtree.root_index, &previous_leaf)?
                        {
                            log_trivial_modification!(subtree.root_index, previous_leaf);
                        }
                        // If previous values of modified leaves are requested, add this leaf.
                        if let Some(ref mut leaves) = previous_leaves {
                            leaves.insert(subtree.root_index, previous_leaf);
                        }
                    }
                }
            }
        }
        current_subtrees = next_subtrees;
        next_subtrees = Vec::new();
    }
    Ok(())
}

async fn get_roots_from_storage<'a, L: Leaf + IndexLayoutLeaf, Hasher: TreeHashFunction<L>>(
    subtrees: &Vec<SubTree<'a>>,
    storage: &mut impl Storage,
    key_context: &KeyContext,
) -> Result<Vec<IndexDbFilledNode<L>>, TraversalError> {
    let mut subtrees_roots = vec![];
    let db_keys: Vec<DbKey> = subtrees
        .iter()
        .map(|subtree| db_key_from_node_index_and_context(subtree.root_index, key_context))
        .collect();

    let db_vals = storage.mget(&db_keys.iter().collect::<Vec<&DbKey>>()).await?;
    for ((subtree, optional_val), db_key) in subtrees.iter().zip(db_vals.iter()).zip(db_keys) {
        let Some(val) = optional_val else { Err(StorageError::MissingKey(db_key))? };

        if subtree.is_leaf() {
            subtrees_roots.push(IndexDbFilledNode::deserialize_leaf::<Hasher>(val)?);
        } else {
            subtrees_roots.push(IndexDbFilledNode::deserialize_inner_node(val)?);
        }
    }
    Ok(subtrees_roots)
}

pub async fn create_original_skeleton_tree<
    'a,
    L: Leaf + IndexLayoutLeaf,
    Hasher: TreeHashFunction<L>,
>(
    storage: &mut impl Storage,
    root_hash: HashOutput,
    sorted_leaf_indices: SortedLeafIndices<'a>,
    config: &impl OriginalSkeletonTreeConfig<L>,
    leaf_modifications: &LeafModifications<L>,
    key_context: &KeyContext,
) -> OriginalSkeletonTreeResult<OriginalSkeletonTreeImpl<'a>> {
    if sorted_leaf_indices.is_empty() {
        return Ok(OriginalSkeletonTreeImpl::create_unmodified(root_hash));
    }
    if root_hash == HashOutput::ROOT_OF_EMPTY_TREE {
        return Ok(handle_empty_subtree::<L>(sorted_leaf_indices).0);
    }
    let main_subtree = SubTree { sorted_leaf_indices, root_index: NodeIndex::ROOT };
    let mut skeleton_tree = OriginalSkeletonTreeImpl { nodes: HashMap::new(), sorted_leaf_indices };
    fetch_nodes::<L, Hasher>(
        &mut skeleton_tree,
        vec![main_subtree],
        storage,
        leaf_modifications,
        config,
        None,
        key_context,
    )
    .await?;
    Ok(skeleton_tree)
}

impl<S: Storage> TrieReader for IndexDb<S> {
    async fn create_contracts_trie<'a>(
        &mut self,
        root_hash: HashOutput,
        sorted_leaf_indices: SortedLeafIndices<'a>,
    ) -> ForestResult<(OriginalSkeletonTreeImpl<'a>, HashMap<NodeIndex, ContractState>)> {
        if sorted_leaf_indices.is_empty() {
            let unmodified = OriginalSkeletonTreeImpl::create_unmodified(root_hash);
            return Ok((unmodified, HashMap::new()));
        }
        if root_hash == HashOutput::ROOT_OF_EMPTY_TREE {
            return Ok(handle_empty_subtree(sorted_leaf_indices));
        }
        let main_subtree = SubTree { sorted_leaf_indices, root_index: NodeIndex::ROOT };
        let mut skeleton_tree =
            OriginalSkeletonTreeImpl { nodes: HashMap::new(), sorted_leaf_indices };
        let mut leaves = HashMap::new();
        let key_context = KeyContext { trie_type: TrieType::ContractsTrie };
        fetch_nodes::<ContractState, TreeHashFunctionImpl>(
            &mut skeleton_tree,
            vec![main_subtree],
            &mut self.storage,
            &HashMap::new(),
            &OriginalSkeletonContractsTrieConfig::new(),
            Some(&mut leaves),
            &key_context,
        )
        .await?;
        Ok((skeleton_tree, leaves))
    }

    async fn create_storage_tries<'a>(
        &mut self,
        actual_storage_updates: &HashMap<ContractAddress, LeafModifications<StarknetStorageValue>>,
        original_contracts_trie_leaves: &HashMap<NodeIndex, ContractState>,
        config: &impl Config,
        storage_tries_sorted_indices: &HashMap<ContractAddress, SortedLeafIndices<'a>>,
    ) -> ForestResult<HashMap<ContractAddress, OriginalSkeletonTreeImpl<'a>>> {
        let mut storage_tries = HashMap::new();
        for (address, updates) in actual_storage_updates {
            let sorted_leaf_indices = storage_tries_sorted_indices
                .get(address)
                .ok_or(ForestError::MissingSortedLeafIndices(*address))?;
            let contract_state = original_contracts_trie_leaves
                .get(&contract_address_into_node_index(address))
                .ok_or(ForestError::MissingContractCurrentState(*address))?;
            let config =
                OriginalSkeletonStorageTrieConfig::new(config.warn_on_trivial_modifications());

            let original_skeleton =
                create_original_skeleton_tree::<StarknetStorageValue, TreeHashFunctionImpl>(
                    &mut self.storage,
                    contract_state.storage_root_hash,
                    *sorted_leaf_indices,
                    &config,
                    updates,
                    &KeyContext { trie_type: TrieType::StorageTrie(*address.0) },
                )
                .await?;
            storage_tries.insert(*address, original_skeleton);
        }
        Ok(storage_tries)
    }

    async fn create_classes_trie<'a>(
        &mut self,
        actual_classes_updates: &LeafModifications<CompiledClassHash>,
        classes_trie_root_hash: HashOutput,
        config: &impl Config,
        contracts_trie_sorted_indices: SortedLeafIndices<'a>,
    ) -> ForestResult<OriginalSkeletonTreeImpl<'a>> {
        let config = OriginalSkeletonClassesTrieConfig::new(config.warn_on_trivial_modifications());

        Ok(create_original_skeleton_tree::<CompiledClassHash, TreeHashFunctionImpl>(
            &mut self.storage,
            classes_trie_root_hash,
            contracts_trie_sorted_indices,
            &config,
            actual_classes_updates,
            &KeyContext { trie_type: TrieType::ClassesTrie },
        )
        .await?)
    }
}

impl<S: Storage> ForestWriter for IndexDb<S> {
    async fn write(&mut self, filled_forest: &FilledForest) -> usize {
        filled_forest.write_to_storage(&mut self.storage).await
    }
}
