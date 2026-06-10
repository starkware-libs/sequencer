use std::collections::HashMap;

use blockifier::state::accessed_keys::AccessedKeys;
use starknet_api::core::ContractAddress;
use starknet_api::hash::{HashOutput, StateRoots};
use starknet_committer::block_committer::input::{
    contract_address_into_node_index,
    try_node_index_into_contract_address,
};
use starknet_committer::db::facts_db::create_facts_tree::get_leaves;
use starknet_committer::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use starknet_committer::patricia_merkle_tree::tree::fetch_previous_and_new_patricia_paths;
use starknet_committer::patricia_merkle_tree::types::RootHashes;
// `CommitmentInfo` and `StateCommitmentInfos` were relocated to `starknet_committer` so that
// lower layers (e.g. `apollo_storage`) can store them without depending on `starknet_os`. They
// are re-exported here to keep this module's public API stable for OS consumers.
pub use starknet_committer::patricia_merkle_tree::types::{CommitmentInfo, StateCommitmentInfos};
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::flatten_preimages;
use starknet_patricia::patricia_merkle_tree::original_skeleton_tree::errors::OriginalSkeletonTreeError;
use starknet_patricia::patricia_merkle_tree::traversal::TraversalError;
use starknet_patricia::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices, SubTreeHeight};
use starknet_patricia_storage::db_object::EmptyKeyContext;
use starknet_patricia_storage::map_storage::MapStorage;
use thiserror::Error;

/// Error type for commitment infos creation.
#[derive(Debug, Error)]
pub enum CommitmentInfosError {
    #[error("Invalid node index: {0}")]
    InvalidNodeIndex(String),
    #[error("Failed to get leaves: {0}")]
    GetLeaves(#[from] OriginalSkeletonTreeError),
    #[error("Failed to fetch storage proofs: {0}")]
    FetchStorageProofs(#[from] TraversalError),
}

/// Creates the commitment infos for the OS from previous and new state roots and the
/// keys that were read during execution.
// TODO(ItamarS): Temporary — to be deleted once the committer builds `StateCommitmentInfos` from
// its own storage; tests against that new committer API will be added then. Kept here (as a free
// function rather than an inherent method) because the struct now lives in `starknet_committer`
// and the orphan rule forbids an inherent impl on a foreign type.
pub async fn build_state_commitment_infos(
    previous_state_roots: &StateRoots,
    new_state_roots: &StateRoots,
    commitments: &mut MapStorage,
    accessed_keys: &AccessedKeys,
) -> Result<StateCommitmentInfos, CommitmentInfosError> {
    let addresses: Vec<ContractAddress> =
        accessed_keys.accessed_contracts.iter().copied().collect();

    let previous_storage_roots =
        get_storage_roots(&addresses, previous_state_roots.contracts_trie_root_hash, commitments)
            .await?;
    let new_storage_roots =
        get_storage_roots(&addresses, new_state_roots.contracts_trie_root_hash, commitments)
            .await?;

    let storage_proofs = fetch_previous_and_new_patricia_paths(
        commitments,
        RootHashes {
            previous_root_hash: previous_state_roots.classes_trie_root_hash,
            new_root_hash: new_state_roots.classes_trie_root_hash,
        },
        RootHashes {
            previous_root_hash: previous_state_roots.contracts_trie_root_hash,
            new_root_hash: new_state_roots.contracts_trie_root_hash,
        },
        accessed_keys,
    )
    .await?;

    let contracts_trie_commitment_info = CommitmentInfo {
        previous_root: previous_state_roots.contracts_trie_root_hash,
        updated_root: new_state_roots.contracts_trie_root_hash,
        tree_height: SubTreeHeight::ACTUAL_HEIGHT,
        commitment_facts: flatten_preimages(&storage_proofs.contracts_trie_proof.nodes),
    };
    let classes_trie_commitment_info = CommitmentInfo {
        previous_root: previous_state_roots.classes_trie_root_hash,
        updated_root: new_state_roots.classes_trie_root_hash,
        tree_height: SubTreeHeight::ACTUAL_HEIGHT,
        commitment_facts: flatten_preimages(&storage_proofs.classes_trie_proof),
    };
    let storage_tries_commitment_infos = previous_storage_roots
        .iter()
        .map(|(address, previous_root_hash)| {
            // Not all contracts in `previous_storage_roots` have storage proofs. For
            // example, a contract that only had its nonce changed.
            let storage_proof = flatten_preimages(
                storage_proofs
                    .contracts_trie_storage_proofs
                    .get(address)
                    .unwrap_or(&HashMap::new()),
            );
            (
                *address,
                CommitmentInfo {
                    previous_root: *previous_root_hash,
                    updated_root: new_storage_roots[address],
                    tree_height: SubTreeHeight::ACTUAL_HEIGHT,
                    commitment_facts: storage_proof,
                },
            )
        })
        .collect();

    Ok(StateCommitmentInfos {
        contracts_trie_commitment_info,
        classes_trie_commitment_info,
        storage_tries_commitment_infos,
    })
}

/// Fetches the storage root hash of each contract from the contracts trie at the given root.
async fn get_storage_roots(
    contract_addresses: &[ContractAddress],
    contracts_trie_root: HashOutput,
    commitments: &mut MapStorage,
) -> Result<HashMap<ContractAddress, HashOutput>, CommitmentInfosError> {
    let mut contract_leaf_indices: Vec<NodeIndex> =
        contract_addresses.iter().map(contract_address_into_node_index).collect();
    let sorted_contract_leaf_indices = SortedLeafIndices::new(&mut contract_leaf_indices);
    let contract_states: HashMap<NodeIndex, ContractState> = get_leaves(
        commitments,
        contracts_trie_root,
        sorted_contract_leaf_indices,
        &EmptyKeyContext,
    )
    .await?;

    contract_states
        .into_iter()
        .map(|(idx, contract_state)| {
            let address = try_node_index_into_contract_address(&idx)
                .map_err(CommitmentInfosError::InvalidNodeIndex)?;
            Ok((address, contract_state.storage_root_hash))
        })
        .collect()
}
