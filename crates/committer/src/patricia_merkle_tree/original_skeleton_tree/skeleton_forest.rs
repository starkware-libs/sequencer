use crate::block_committer::input::ContractAddress;
use crate::block_committer::input::Input;
use crate::block_committer::input::StarknetStorageKey;
use crate::block_committer::input::StarknetStorageValue;
use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::node_data::leaf::ContractState;
use crate::patricia_merkle_tree::node_data::leaf::LeafData;
use crate::patricia_merkle_tree::original_skeleton_tree::errors::OriginalSkeletonTreeError;
use crate::patricia_merkle_tree::original_skeleton_tree::tree::OriginalSkeletonTree;
use crate::patricia_merkle_tree::original_skeleton_tree::tree::OriginalSkeletonTreeResult;
use crate::patricia_merkle_tree::types::NodeIndex;
use crate::patricia_merkle_tree::types::TreeHeight;
use crate::storage::storage_trait::Storage;
use core::marker::PhantomData;
use std::collections::HashMap;
use std::collections::HashSet;

#[cfg(test)]
#[path = "skeleton_forest_test.rs"]
pub mod skeleton_forest_test;

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct OriginalSkeletonForest<
    L: LeafData + std::clone::Clone,
    T: OriginalSkeletonTree<L>,
> {
    // TODO(Nimrod): Add compiled class tree.
    #[allow(dead_code)]
    global_state_tree: T,
    #[allow(dead_code)]
    contract_states: HashMap<ContractAddress, T>,
    leaf_data: PhantomData<L>,
}

impl<L: LeafData + std::clone::Clone, T: OriginalSkeletonTree<L>> OriginalSkeletonForest<L, T> {
    pub(crate) fn new(global_state_tree: T, contract_states: HashMap<ContractAddress, T>) -> Self {
        Self {
            global_state_tree,
            contract_states,
            leaf_data: PhantomData,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn create_original_skeleton_forest<S: Storage>(
        input: Input,
    ) -> OriginalSkeletonTreeResult<OriginalSkeletonForest<L, T>> {
        let storage = S::from(input.storage);
        let accessed_addresses = input.state_diff.accessed_addresses();
        let global_state_tree = Self::create_global_state_tree(
            &accessed_addresses,
            input.global_tree_root_hash,
            &storage,
            input.tree_heights,
        )?;
        let contract_states = Self::create_lower_trees_skeleton(
            accessed_addresses,
            &input.current_contract_state_leaves,
            &input.state_diff.storage_updates,
            &storage,
            input.tree_heights,
        )?;
        Ok(OriginalSkeletonForest::new(
            global_state_tree,
            contract_states,
        ))
    }
    fn create_global_state_tree<S: Storage>(
        accessed_addresses: &HashSet<&ContractAddress>,
        global_tree_root_hash: HashOutput,
        storage: &S,
        tree_height: TreeHeight,
    ) -> OriginalSkeletonTreeResult<T> {
        let mut sorted_leaf_indices: Vec<NodeIndex> = accessed_addresses
            .iter()
            .map(|address| NodeIndex::from_contract_address(address, &tree_height))
            .collect();
        sorted_leaf_indices.sort();
        T::create_tree(
            storage,
            &sorted_leaf_indices,
            global_tree_root_hash,
            tree_height,
        )
    }
    fn create_lower_trees_skeleton<S: Storage>(
        accessed_addresses: HashSet<&ContractAddress>,
        current_contract_state_leaves: &HashMap<ContractAddress, ContractState>,
        storage_updates: &HashMap<
            ContractAddress,
            HashMap<StarknetStorageKey, StarknetStorageValue>,
        >,
        storage: &S,
        tree_height: TreeHeight,
    ) -> OriginalSkeletonTreeResult<HashMap<ContractAddress, T>> {
        let mut contract_states = HashMap::new();
        for address in accessed_addresses {
            let mut sorted_leaf_indices: Vec<NodeIndex> = storage_updates
                .get(address)
                .unwrap_or(&HashMap::new())
                .keys()
                .map(|key| NodeIndex::from_starknet_storage_key(key, &tree_height))
                .collect();
            sorted_leaf_indices.sort();
            let contract_state = current_contract_state_leaves
                .get(address)
                .ok_or_else(|| OriginalSkeletonTreeError::LowerTreeCommitmentError(*address))?;
            let original_skeleton = T::create_tree(
                storage,
                &sorted_leaf_indices,
                contract_state.storage_root_hash,
                tree_height,
            )?;
            contract_states.insert(*address, original_skeleton);
        }
        Ok(contract_states)
    }
}
