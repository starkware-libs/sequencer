use std::collections::HashMap;

use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_patricia::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices};
use starknet_patricia_storage::map_storage::BorrowedMapStorage;
use starknet_patricia_storage::storage_trait::{DbKey, DbValue};
use tracing::{info, warn};

use crate::block_committer::errors::BlockCommitmentError;
use crate::block_committer::input::{
    contract_address_into_node_index,
    Config,
    ConfigImpl,
    Input,
    StateDiff,
};
use crate::forest::filled_forest::FilledForest;
use crate::forest::original_skeleton_forest::{ForestSortedIndices, OriginalSkeletonForest};
use crate::forest::updated_skeleton_forest::UpdatedSkeletonForest;
use crate::hash_function::hash::TreeHashFunctionImpl;
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use crate::patricia_merkle_tree::types::class_hash_into_node_index;

type BlockCommitmentResult<T> = Result<T, BlockCommitmentError>;

pub async fn commit_block(
    input: Input<ConfigImpl>,
    storage: &mut HashMap<DbKey, DbValue>,
) -> BlockCommitmentResult<FilledForest> {
    let (mut storage_tries_indices, mut contracts_trie_indices, mut classes_trie_indices) =
        get_all_modified_indices(&input.state_diff);
    let forest_sorted_indices = ForestSortedIndices {
        storage_tries_sorted_indices: storage_tries_indices
            .iter_mut()
            .map(|(address, indices)| (*address, SortedLeafIndices::new(indices)))
            .collect(),
        contracts_trie_sorted_indices: SortedLeafIndices::new(&mut contracts_trie_indices),
        classes_trie_sorted_indices: SortedLeafIndices::new(&mut classes_trie_indices),
    };
    let actual_storage_updates = input.state_diff.actual_storage_updates();
    let actual_classes_updates = input.state_diff.actual_classes_updates();
    let (mut original_forest, original_contracts_trie_leaves) = OriginalSkeletonForest::create(
        BorrowedMapStorage { storage },
        input.contracts_trie_root_hash,
        input.classes_trie_root_hash,
        &actual_storage_updates,
        &actual_classes_updates,
        &forest_sorted_indices,
        &input.config,
    )?;
    info!("Original skeleton forest created successfully.");

    if input.config.warn_on_trivial_modifications() {
        check_trivial_nonce_and_class_hash_updates(
            &original_contracts_trie_leaves,
            &input.state_diff.address_to_class_hash,
            &input.state_diff.address_to_nonce,
        );
    }

    let updated_forest = UpdatedSkeletonForest::create(
        &mut original_forest,
        &input.state_diff.skeleton_classes_updates(),
        &input.state_diff.skeleton_storage_updates(),
        &original_contracts_trie_leaves,
        &input.state_diff.address_to_class_hash,
        &input.state_diff.address_to_nonce,
    )?;
    info!("Updated skeleton forest created successfully.");

    let filled_forest = FilledForest::create::<TreeHashFunctionImpl>(
        updated_forest,
        actual_storage_updates,
        actual_classes_updates,
        &original_contracts_trie_leaves,
        &input.state_diff.address_to_class_hash,
        &input.state_diff.address_to_nonce,
    )
    .await?;
    info!("Filled forest created successfully.");

    Ok(filled_forest)
}

/// Compares the previous state's nonce and class hash with the given in the state diff.
/// In case of trivial update, logs out a warning for trivial state diff update.
fn check_trivial_nonce_and_class_hash_updates(
    original_contracts_trie_leaves: &HashMap<NodeIndex, ContractState>,
    address_to_class_hash: &HashMap<ContractAddress, ClassHash>,
    address_to_nonce: &HashMap<ContractAddress, Nonce>,
) {
    for (address, nonce) in address_to_nonce.iter() {
        if original_contracts_trie_leaves
            .get(&contract_address_into_node_index(address))
            .is_some_and(|previous_contract_state| previous_contract_state.nonce == *nonce)
        {
            warn!("Encountered a trivial nonce update of contract {:?}", address)
        }
    }

    for (address, class_hash) in address_to_class_hash.iter() {
        if original_contracts_trie_leaves
            .get(&contract_address_into_node_index(address))
            .is_some_and(|previous_contract_state| {
                previous_contract_state.class_hash == *class_hash
            })
        {
            warn!("Encountered a trivial class hash update of contract {:?}", address)
        }
    }
}

type StorageTriesIndices = HashMap<ContractAddress, Vec<NodeIndex>>;
type ContractsTrieIndices = Vec<NodeIndex>;
type ClassesTrieIndices = Vec<NodeIndex>;

/// Returns all modified indices in the given state diff.
pub(crate) fn get_all_modified_indices(
    state_diff: &StateDiff,
) -> (StorageTriesIndices, ContractsTrieIndices, ClassesTrieIndices) {
    let accessed_addresses = state_diff.accessed_addresses();
    let contracts_trie_indices: Vec<NodeIndex> = accessed_addresses
        .iter()
        .map(|address| contract_address_into_node_index(address))
        .collect();
    let classes_trie_indices: Vec<NodeIndex> = state_diff
        .class_hash_to_compiled_class_hash
        .keys()
        .map(class_hash_into_node_index)
        .collect();
    let storage_tries_indices: HashMap<ContractAddress, Vec<NodeIndex>> = accessed_addresses
        .iter()
        .map(|address| {
            let indices: Vec<NodeIndex> = match state_diff.storage_updates.get(address) {
                Some(updates) => updates.keys().map(NodeIndex::from).collect(),
                None => Vec::new(),
            };
            (**address, indices)
        })
        .collect();
    (storage_tries_indices, contracts_trie_indices, classes_trie_indices)
}
