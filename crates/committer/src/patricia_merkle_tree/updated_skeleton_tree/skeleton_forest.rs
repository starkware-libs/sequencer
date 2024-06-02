use std::collections::HashMap;

use crate::block_committer::input::ContractAddress;
use crate::felt::Felt;
use crate::forest_errors::{ForestError, ForestResult};
use crate::patricia_merkle_tree::filled_tree::node::{ClassHash, Nonce};
use crate::patricia_merkle_tree::node_data::leaf::{
    ContractState, LeafModifications, SkeletonLeaf,
};
use crate::patricia_merkle_tree::original_skeleton_tree::skeleton_forest::OriginalSkeletonForestImpl;
use crate::patricia_merkle_tree::original_skeleton_tree::tree::OriginalSkeletonTree;
use crate::patricia_merkle_tree::types::{NodeIndex, TreeHeight};
use crate::patricia_merkle_tree::updated_skeleton_tree::tree::UpdatedSkeletonTree;

#[allow(dead_code)]
pub(crate) struct UpdatedSkeletonForestImpl<T: UpdatedSkeletonTree> {
    #[allow(dead_code)]
    pub(crate) classes_trie: T,
    #[allow(dead_code)]
    pub(crate) contracts_trie: T,
    #[allow(dead_code)]
    pub(crate) storage_tries: HashMap<ContractAddress, T>,
}

pub(crate) trait UpdatedSkeletonForest<T: OriginalSkeletonTree> {
    fn create(
        original_skeleton_forest: &mut OriginalSkeletonForestImpl<T>,
        class_hash_leaf_modifications: &LeafModifications<SkeletonLeaf>,
        storage_updates: &HashMap<ContractAddress, LeafModifications<SkeletonLeaf>>,
        current_contract_state_leaves: &HashMap<ContractAddress, ContractState>,
        address_to_class_hash: &HashMap<ContractAddress, ClassHash>,
        address_to_nonce: &HashMap<ContractAddress, Nonce>,
        tree_heights: TreeHeight,
    ) -> ForestResult<Self>
    where
        Self: std::marker::Sized;
}

impl<T: OriginalSkeletonTree, U: UpdatedSkeletonTree> UpdatedSkeletonForest<T>
    for UpdatedSkeletonForestImpl<U>
{
    fn create(
        original_skeleton_forest: &mut OriginalSkeletonForestImpl<T>,
        class_hash_leaf_modifications: &LeafModifications<SkeletonLeaf>,
        storage_updates: &HashMap<ContractAddress, LeafModifications<SkeletonLeaf>>,
        current_contracts_trie_leaves: &HashMap<ContractAddress, ContractState>,
        address_to_class_hash: &HashMap<ContractAddress, ClassHash>,
        address_to_nonce: &HashMap<ContractAddress, Nonce>,
        tree_heights: TreeHeight,
    ) -> ForestResult<Self>
    where
        Self: std::marker::Sized,
    {
        // Classes trie.
        let classes_trie = U::create(
            &mut original_skeleton_forest.classes_trie,
            class_hash_leaf_modifications,
        )?;

        // Storage tries.
        let mut contracts_trie_leaves = HashMap::new();
        let mut storage_tries = HashMap::new();

        for (address, updates) in storage_updates {
            let original_storage_trie = original_skeleton_forest
                .storage_tries
                .get_mut(address)
                .ok_or(ForestError::MissingOriginalSkeleton(*address))?;

            let updated_storage_trie = U::create(original_storage_trie, updates)?;
            let storage_trie_becomes_empty = updated_storage_trie.is_empty();

            storage_tries.insert(*address, updated_storage_trie);

            let current_leaf = current_contracts_trie_leaves
                .get(address)
                .ok_or(ForestError::MissingContractCurrentState(*address))?;

            let skeleton_leaf = Self::updated_contract_skeleton_leaf(
                address_to_nonce.get(address),
                address_to_class_hash.get(address),
                current_leaf,
                storage_trie_becomes_empty,
            );
            contracts_trie_leaves.insert(
                NodeIndex::from_contract_address(address, &tree_heights),
                skeleton_leaf,
            );
        }

        // Contracts trie.
        let contracts_trie = U::create(
            &mut original_skeleton_forest.contracts_trie,
            &contracts_trie_leaves,
        )?;

        Ok(Self {
            classes_trie,
            contracts_trie,
            storage_tries,
        })
    }
}

impl<U: UpdatedSkeletonTree> UpdatedSkeletonForestImpl<U> {
    /// Given the previous contract state, whether the contract's storage has become empty or not,
    /// optional new nonce & new class hash, the function creates a skeleton leaf.
    fn updated_contract_skeleton_leaf(
        new_nonce: Option<&Nonce>,
        new_class_hash: Option<&ClassHash>,
        previous_state: &ContractState,
        storage_becomes_empty: bool,
    ) -> SkeletonLeaf {
        let actual_new_nonce = new_nonce.unwrap_or(&previous_state.nonce);
        let actual_new_class_hash = new_class_hash.unwrap_or(&previous_state.class_hash);
        if storage_becomes_empty
            && actual_new_nonce.0 == Felt::ZERO
            && actual_new_class_hash.0 == Felt::ZERO
        {
            SkeletonLeaf::Zero
        } else {
            SkeletonLeaf::NonZero
        }
    }
}
