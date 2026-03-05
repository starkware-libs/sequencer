use std::collections::HashMap;

use blockifier::state::cached_state::StateChangesKeys;
use starknet_api::core::{ClassHash, ContractAddress};
use starknet_api::hash::{HashOutput, StateRoots};
use starknet_committer::block_committer::input::{
    contract_address_into_node_index,
    try_node_index_into_contract_address,
    StarknetStorageKey,
};
use starknet_committer::db::facts_db::create_facts_tree::get_leaves;
use starknet_committer::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use starknet_committer::patricia_merkle_tree::tree::fetch_previous_and_new_patricia_paths;
use starknet_committer::patricia_merkle_tree::types::{RootHashes, StarknetForestProofs};
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::flatten_preimages;
use starknet_patricia::patricia_merkle_tree::original_skeleton_tree::errors::OriginalSkeletonTreeError;
use starknet_patricia::patricia_merkle_tree::traversal::TraversalError;
use starknet_patricia::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices, SubTreeHeight};
use starknet_patricia_storage::db_object::EmptyKeyContext;
use starknet_patricia_storage::map_storage::MapStorage;
use starknet_types_core::felt::Felt;
use thiserror::Error;

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize))]
#[cfg_attr(feature = "deserialize", serde(deny_unknown_fields))]
#[derive(Debug)]
pub struct CommitmentInfo {
    pub previous_root: HashOutput,
    pub updated_root: HashOutput,
    pub tree_height: SubTreeHeight,
    // TODO(Dori, 1/8/2025): The value type here should probably be more specific (NodeData<L> for
    //   L: Leaf). This poses a problem in deserialization, as a serialized edge node and a
    //   serialized contract state leaf are both currently vectors of 3 field elements; as the
    //   semantics of the values are unimportant for the OS commitments, we make do with a vector
    //   of field elements as values for now.
    pub commitment_facts: HashMap<HashOutput, Vec<Felt>>,
}

#[cfg(any(feature = "testing", test))]
impl Default for CommitmentInfo {
    fn default() -> CommitmentInfo {
        CommitmentInfo {
            previous_root: HashOutput::default(),
            updated_root: HashOutput::default(),
            tree_height: SubTreeHeight::ACTUAL_HEIGHT,
            commitment_facts: HashMap::default(),
        }
    }
}

// TODO(Aviv): Use this struct in `OsBlockInput`
/// Contains all commitment information for a block's state trees.
pub struct StateCommitmentInfos {
    pub contracts_trie_commitment_info: CommitmentInfo,
    pub classes_trie_commitment_info: CommitmentInfo,
    pub storage_tries_commitment_infos: HashMap<ContractAddress, CommitmentInfo>,
}

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

impl StateCommitmentInfos {
    /// Creates the commitment infos for the OS from previous and new state roots and the
    /// keys that were read during execution.
    pub async fn new(
        previous_state_roots: &StateRoots,
        new_state_roots: &StateRoots,
        commitments: &mut MapStorage,
        initial_reads_keys: &StateChangesKeys,
    ) -> Result<Self, CommitmentInfosError> {
        let addresses: Vec<ContractAddress> =
            initial_reads_keys.modified_contracts.iter().copied().collect();

        let previous_storage_roots = get_storage_roots(
            &addresses,
            previous_state_roots.contracts_trie_root_hash,
            commitments,
        )
        .await?;
        let new_storage_roots =
            get_storage_roots(&addresses, new_state_roots.contracts_trie_root_hash, commitments)
                .await?;

        let storage_proofs = fetch_storage_proofs_from_state_changes_keys(
            initial_reads_keys,
            commitments,
            RootHashes {
                previous_root_hash: previous_state_roots.classes_trie_root_hash,
                new_root_hash: new_state_roots.classes_trie_root_hash,
            },
            RootHashes {
                previous_root_hash: previous_state_roots.contracts_trie_root_hash,
                new_root_hash: new_state_roots.contracts_trie_root_hash,
            },
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

        Ok(Self {
            contracts_trie_commitment_info,
            classes_trie_commitment_info,
            storage_tries_commitment_infos,
        })
    }
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

async fn fetch_storage_proofs_from_state_changes_keys(
    initial_reads_keys: &StateChangesKeys,
    storage: &mut MapStorage,
    classes_trie_root_hashes: RootHashes,
    contracts_trie_root_hashes: RootHashes,
) -> Result<StarknetForestProofs, CommitmentInfosError> {
    let class_hashes: Vec<ClassHash> =
        initial_reads_keys.compiled_class_hash_keys.iter().cloned().collect();
    let contract_addresses =
        &initial_reads_keys.modified_contracts.iter().cloned().collect::<Vec<_>>();
    let contract_storage_keys = initial_reads_keys.storage_keys.iter().fold(
        HashMap::<ContractAddress, Vec<StarknetStorageKey>>::new(),
        |mut acc, (address, key)| {
            acc.entry(*address).or_default().push(StarknetStorageKey(*key));
            acc
        },
    );

    Ok(fetch_previous_and_new_patricia_paths(
        storage,
        classes_trie_root_hashes,
        contracts_trie_root_hashes,
        &class_hashes,
        contract_addresses,
        &contract_storage_keys,
    )
    .await?)
}
