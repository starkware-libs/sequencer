use std::collections::HashMap;

use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_patricia::patricia_merkle_tree::node_data::leaf::{LeafModifications, SkeletonLeaf};
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;
use starknet_patricia::patricia_merkle_tree::updated_skeleton_tree::tree::{
    UpdatedSkeletonTree,
    UpdatedSkeletonTreeImpl,
};
use starknet_types_core::felt::Felt;

use crate::block_committer::input::contract_address_into_node_index;
use crate::forest::forest_errors::{ForestError, ForestResult};
use crate::forest::original_skeleton_forest::OriginalSkeletonForest;
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;

pub(crate) struct UpdatedSkeletonForest {
    pub(crate) classes_trie: UpdatedSkeletonTreeImpl,
    pub(crate) contracts_trie: UpdatedSkeletonTreeImpl,
    pub(crate) storage_tries: HashMap<ContractAddress, UpdatedSkeletonTreeImpl>,
}

impl UpdatedSkeletonForest {
    pub(crate) fn create(
        original_skeleton_forest: &OriginalSkeletonForest<'_>,
        class_hash_leaf_modifications: &LeafModifications<SkeletonLeaf>,
        storage_updates: &HashMap<ContractAddress, LeafModifications<SkeletonLeaf>>,
        original_contracts_trie_leaves: &HashMap<NodeIndex, ContractState>,
        address_to_class_hash: &HashMap<ContractAddress, ClassHash>,
        address_to_nonce: &HashMap<ContractAddress, Nonce>,
    ) -> ForestResult<Self>
    where
        Self: std::marker::Sized,
    {
        // Classes trie.
        let classes_trie = UpdatedSkeletonTreeImpl::create(
            &original_skeleton_forest.classes_trie,
            class_hash_leaf_modifications,
        )?;

        // Storage tries.
        let mut contracts_trie_leaves = HashMap::new();
        let mut storage_tries = HashMap::new();

        for (address, updates) in storage_updates {
            let address_as_node_index = contract_address_into_node_index(address);
            let original_storage_trie = original_skeleton_forest
                .storage_tries
                .get(address)
                .ok_or(ForestError::MissingOriginalSkeleton(*address))?;

            let updated_storage_trie =
                UpdatedSkeletonTreeImpl::create(original_storage_trie, updates)?;
            let storage_trie_becomes_empty = updated_storage_trie.is_empty();

            storage_tries.insert(*address, updated_storage_trie);

            let current_leaf = original_contracts_trie_leaves
                .get(&address_as_node_index)
                .ok_or(ForestError::MissingContractCurrentState(*address))?;

            let skeleton_leaf = Self::updated_contract_skeleton_leaf(
                address_to_nonce.get(address),
                address_to_class_hash.get(address),
                current_leaf,
                storage_trie_becomes_empty,
            );
            contracts_trie_leaves.insert(address_as_node_index, skeleton_leaf);
        }

        // Contracts trie.
        let contracts_trie = UpdatedSkeletonTreeImpl::create(
            &original_skeleton_forest.contracts_trie,
            &contracts_trie_leaves,
        )?;

        Ok(Self { classes_trie, contracts_trie, storage_tries })
    }

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
