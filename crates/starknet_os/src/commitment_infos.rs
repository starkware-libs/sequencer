use std::collections::HashMap;

use blockifier::state::cached_state::StateChangesKeys;
use starknet_api::core::{ClassHash, ContractAddress};
use starknet_api::hash::{HashOutput, StateRoots};
use starknet_api::state::StorageKey;
use starknet_committer::block_committer::input::{
    try_node_index_into_contract_address,
    try_node_index_into_patricia_key,
    StarknetStorageKey,
    StarknetStorageValue,
};
use starknet_committer::db::facts_db::create_facts_tree::get_leaves;
use starknet_committer::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use starknet_committer::patricia_merkle_tree::tree::fetch_previous_and_new_patricia_paths;
use starknet_committer::patricia_merkle_tree::types::{RootHashes, StarknetForestProofs};
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::flatten_preimages;
use starknet_patricia::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices, SubTreeHeight};
use starknet_patricia_storage::db_object::EmptyKeyContext;
use starknet_patricia_storage::map_storage::MapStorage;
use starknet_types_core::felt::Felt;

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

/// Creates the commitment infos for the OS.
pub async fn create_commitment_infos(
    previous_state_roots: &StateRoots,
    new_state_roots: &StateRoots,
    commitments: &mut MapStorage,
    initial_reads_keys: &StateChangesKeys,
) -> StateCommitmentInfos {
    let (previous_contract_states, new_storage_roots) = get_previous_states_and_new_storage_roots(
        initial_reads_keys.modified_contracts.iter().copied(),
        previous_state_roots.contracts_trie_root_hash,
        new_state_roots.contracts_trie_root_hash,
        commitments,
    )
    .await;
    let mut address_to_previous_storage_root_hash = HashMap::new();
    for (address, contract_state) in previous_contract_states.into_iter() {
        let address = try_node_index_into_contract_address(&address).unwrap();
        address_to_previous_storage_root_hash.insert(address, contract_state.storage_root_hash);
    }

    let mut storage = HashMap::new();
    for address in &initial_reads_keys.modified_contracts {
        let mut storage_keys_indices: Vec<NodeIndex> =
            initial_reads_keys
                .storage_keys
                .iter()
                .filter_map(|(add, key)| {
                    if add == address { Some(NodeIndex::from_leaf_felt(&key.0)) } else { None }
                })
                .collect();
        let sorted_leaf_indices = SortedLeafIndices::new(&mut storage_keys_indices);
        let previous_storage_leaves: HashMap<NodeIndex, StarknetStorageValue> = get_leaves(
            commitments,
            address_to_previous_storage_root_hash[address],
            sorted_leaf_indices,
            address,
        )
        .await
        .unwrap();
        for (idx, v) in previous_storage_leaves {
            let key = StorageKey(try_node_index_into_patricia_key(&idx).unwrap());
            storage.insert((*address, key), v.0);
        }
    }

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
    .await;
    let contracts_trie_commitment_info = CommitmentInfo {
        previous_root: previous_state_roots.contracts_trie_root_hash,
        updated_root: new_state_roots.contracts_trie_root_hash,
        tree_height: starknet_patricia::patricia_merkle_tree::types::SubTreeHeight::ACTUAL_HEIGHT,
        commitment_facts: flatten_preimages(&storage_proofs.contracts_trie_proof.nodes),
    };
    let classes_trie_commitment_info = CommitmentInfo {
        previous_root: previous_state_roots.classes_trie_root_hash,
        updated_root: new_state_roots.classes_trie_root_hash,
        tree_height: starknet_patricia::patricia_merkle_tree::types::SubTreeHeight::ACTUAL_HEIGHT,
        commitment_facts: flatten_preimages(&storage_proofs.classes_trie_proof),
    };
    let storage_tries_commitment_infos = address_to_previous_storage_root_hash
        .iter()
        .map(|(address, previous_root_hash)| {
            // Not all contracts in `address_to_previous_storage_root_hash` are in
            // `extended_state_diff`. For example a contract that only its Nonce was
            // changed.
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
                    tree_height:
                        starknet_patricia::patricia_merkle_tree::types::SubTreeHeight::ACTUAL_HEIGHT,
                    commitment_facts: storage_proof,
                },
            )
        })
        .collect();

    StateCommitmentInfos {
        contracts_trie_commitment_info,
        classes_trie_commitment_info,
        storage_tries_commitment_infos,
    }
}

pub async fn get_previous_states_and_new_storage_roots<I: Iterator<Item = ContractAddress>>(
    contract_addresses: I,
    previous_contract_trie_root: HashOutput,
    new_contract_trie_root: HashOutput,
    commitments: &mut MapStorage,
) -> (HashMap<NodeIndex, ContractState>, HashMap<ContractAddress, HashOutput>) {
    let mut contract_leaf_indices: Vec<NodeIndex> =
        contract_addresses.map(|address| NodeIndex::from_leaf_felt(&address.0)).collect();

    // Get previous contract state leaves.
    let sorted_contract_leaf_indices = SortedLeafIndices::new(&mut contract_leaf_indices);
    // Get the previous and the new contract states.
    let previous_contract_states = get_leaves(
        commitments,
        previous_contract_trie_root,
        sorted_contract_leaf_indices,
        &EmptyKeyContext,
    )
    .await
    .unwrap();
    let new_contract_states: HashMap<NodeIndex, ContractState> = get_leaves(
        commitments,
        new_contract_trie_root,
        sorted_contract_leaf_indices,
        &EmptyKeyContext,
    )
    .await
    .unwrap();
    let new_contract_roots: HashMap<ContractAddress, HashOutput> = new_contract_states
        .into_iter()
        .map(|(idx, contract_state)| {
            (try_node_index_into_contract_address(&idx).unwrap(), contract_state.storage_root_hash)
        })
        .collect();
    (previous_contract_states, new_contract_roots)
}

async fn fetch_storage_proofs_from_state_changes_keys(
    initial_reads_keys: &StateChangesKeys,
    storage: &mut MapStorage,
    classes_trie_root_hashes: RootHashes,
    contracts_trie_root_hashes: RootHashes,
) -> StarknetForestProofs {
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

    fetch_previous_and_new_patricia_paths(
        storage,
        classes_trie_root_hashes,
        contracts_trie_root_hashes,
        &class_hashes,
        contract_addresses,
        &contract_storage_keys,
    )
    .await
    .unwrap()
}
