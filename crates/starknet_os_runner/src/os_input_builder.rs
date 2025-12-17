//! Build OS input structures from RPC proofs.
//!
//! This module provides functionality to fetch proofs from an RPC endpoint
//! and construct the necessary OS input structures.

use std::collections::HashMap;

use blockifier::state::cached_state::StateMaps;
use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_api::hash::HashOutput;
use starknet_api::state::StorageKey;
use starknet_committer::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use starknet_committer::patricia_merkle_tree::types::{ContractsTrieProof, StarknetForestProofs};
use starknet_os::io::os_input::{CachedStateInput, CommitmentInfo};
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::{
    flatten_preimages,
    Preimage,
    PreimageMap,
};
use starknet_patricia::patricia_merkle_tree::types::SubTreeHeight;
use starknet_rust_core::types::{ContractStorageKeys, Felt, MerkleNode, StorageProof};

/// Result of fetching and converting proofs from RPC.
pub struct RpcProofResult {
    pub forest_proofs: StarknetForestProofs,
    pub contracts_trie_root: HashOutput,
    pub classes_trie_root: HashOutput,
}

/// Commitment information for all tries.
pub struct CommitmentInfos {
    pub contracts_trie: CommitmentInfo,
    pub classes_trie: CommitmentInfo,
    pub storage_tries: HashMap<ContractAddress, CommitmentInfo>,
}

/// Complete OS input built from RPC proofs.
pub struct OsInputFromProofs {
    pub cached_state_input: CachedStateInput,
    pub commitment_infos: CommitmentInfos,
}

/// Builder for constructing OS input from RPC proofs.
///
/// Assumes no state changes occurred (previous state == new state).
pub struct OsInputBuilder {
    proof_result: RpcProofResult,
    state_maps: StateMaps,
}

impl OsInputBuilder {
    /// Create a new builder from proof result and state maps.
    pub fn new(proof_result: RpcProofResult, state_maps: StateMaps) -> Self {
        Self { proof_result, state_maps }
    }

    /// Build the complete OS input.
    pub fn build(self) -> OsInputFromProofs {
        let cached_state_input = self.build_cached_state_input();
        let commitment_infos = self.build_commitment_infos();

        OsInputFromProofs { cached_state_input, commitment_infos }
    }

    fn build_cached_state_input(&self) -> CachedStateInput {
        // Build address_to_class_hash and address_to_nonce from the contract leaves.
        let mut address_to_class_hash = HashMap::new();
        let mut address_to_nonce = HashMap::new();

        for (address, contract_state) in
            &self.proof_result.forest_proofs.contracts_trie_proof.leaves
        {
            address_to_class_hash.insert(*address, contract_state.class_hash);
            address_to_nonce.insert(*address, contract_state.nonce);
        }

        // Build storage from StateMaps - group by contract address.
        let mut storage: HashMap<ContractAddress, HashMap<StorageKey, Felt>> = HashMap::new();
        for ((address, key), value) in &self.state_maps.storage {
            storage.entry(*address).or_default().insert(*key, *value);
        }

        // Build class_hash_to_compiled_class_hash from StateMaps.
        let class_hash_to_compiled_class_hash = self.state_maps.compiled_class_hashes.clone();

        CachedStateInput {
            storage,
            address_to_class_hash,
            address_to_nonce,
            class_hash_to_compiled_class_hash,
        }
    }

    fn build_commitment_infos(&self) -> CommitmentInfos {
        let contracts_trie_root = self.proof_result.contracts_trie_root;
        let classes_trie_root = self.proof_result.classes_trie_root;

        let contracts_trie = CommitmentInfo {
            previous_root: contracts_trie_root,
            updated_root: contracts_trie_root, // No state change.
            tree_height: SubTreeHeight::ACTUAL_HEIGHT,
            commitment_facts: flatten_preimages(
                &self.proof_result.forest_proofs.contracts_trie_proof.nodes,
            ),
        };

        let classes_trie = CommitmentInfo {
            previous_root: classes_trie_root,
            updated_root: classes_trie_root, // No state change.
            tree_height: SubTreeHeight::ACTUAL_HEIGHT,
            commitment_facts: flatten_preimages(
                &self.proof_result.forest_proofs.classes_trie_proof,
            ),
        };

        let storage_tries = self
            .proof_result
            .forest_proofs
            .contracts_trie_proof
            .leaves
            .iter()
            .map(|(address, contract_state)| {
                let storage_proof = self
                    .proof_result
                    .forest_proofs
                    .contracts_trie_storage_proofs
                    .get(address)
                    .map(|p| flatten_preimages(p))
                    .unwrap_or_default();

                let commitment_info = CommitmentInfo {
                    previous_root: contract_state.storage_root_hash,
                    updated_root: contract_state.storage_root_hash, // No state change.
                    tree_height: SubTreeHeight::ACTUAL_HEIGHT,
                    commitment_facts: storage_proof,
                };
                (*address, commitment_info)
            })
            .collect();

        CommitmentInfos { contracts_trie, classes_trie, storage_tries }
    }
}

/// Convert an IndexMap of Felt -> MerkleNode to PreimageMap.
fn index_map_to_preimage_map<S>(nodes: &indexmap::IndexMap<Felt, MerkleNode, S>) -> PreimageMap {
    nodes.iter().map(|(hash, node)| (HashOutput(*hash), Preimage::from(node))).collect()
}

impl RpcProofResult {
    /// Convert a `StorageProof` from RPC to `RpcProofResult`.
    ///
    /// The `contract_addresses` must be in the same order as the storage proofs in the response.
    pub fn from_storage_proof(proof: StorageProof, contract_addresses: &[ContractAddress]) -> Self {
        assert_eq!(
            proof.contracts_storage_proofs.len(),
            contract_addresses.len(),
            "Mismatch between contracts_storage_proofs ({}) and contract_addresses ({})",
            proof.contracts_storage_proofs.len(),
            contract_addresses.len()
        );
        assert_eq!(
            proof.contracts_proof.contract_leaves_data.len(),
            contract_addresses.len(),
            "Mismatch between contract_leaves_data ({}) and contract_addresses ({})",
            proof.contracts_proof.contract_leaves_data.len(),
            contract_addresses.len()
        );

        let contracts_trie_storage_proofs = contract_addresses
            .iter()
            .zip(proof.contracts_storage_proofs.iter())
            .map(|(address, nodes)| (*address, index_map_to_preimage_map(nodes)))
            .collect();

        // Build contract state leaves from contract_leaves_data.
        let leaves: HashMap<ContractAddress, ContractState> = contract_addresses
            .iter()
            .zip(proof.contracts_proof.contract_leaves_data.iter())
            .map(|(address, leaf_data)| {
                let contract_state = ContractState {
                    nonce: Nonce(leaf_data.nonce),
                    class_hash: ClassHash(leaf_data.class_hash),
                    storage_root_hash: HashOutput(leaf_data.storage_root.unwrap_or(Felt::ZERO)),
                };
                (*address, contract_state)
            })
            .collect();

        Self {
            forest_proofs: StarknetForestProofs {
                classes_trie_proof: index_map_to_preimage_map(&proof.classes_proof),
                contracts_trie_proof: ContractsTrieProof {
                    nodes: index_map_to_preimage_map(&proof.contracts_proof.nodes),
                    leaves,
                },
                contracts_trie_storage_proofs,
            },
            contracts_trie_root: HashOutput(proof.global_roots.contracts_tree_root),
            classes_trie_root: HashOutput(proof.global_roots.classes_tree_root),
        }
    }
}

/// Extract query parameters from StateMaps for the RPC call.
pub fn extract_query_params(
    state_maps: &StateMaps,
) -> (Vec<Felt>, Vec<ContractAddress>, Vec<ContractStorageKeys>) {
    // Class hashes: from compiled_class_hashes keys + class_hashes values.
    let class_hashes: Vec<Felt> = state_maps
        .compiled_class_hashes
        .keys()
        .map(|ch| ch.0)
        .chain(state_maps.class_hashes.values().map(|ch| ch.0))
        .collect();

    // Contract addresses: all unique addresses from nonces, class_hashes, and storage.
    let contract_addresses: Vec<ContractAddress> =
        state_maps.get_contract_addresses().into_iter().collect();

    // Storage keys grouped by contract address.
    let mut storage_by_address: HashMap<ContractAddress, Vec<Felt>> = HashMap::new();
    for (address, key) in state_maps.storage.keys() {
        storage_by_address.entry(*address).or_default().push(*key.0.key());
    }

    let contract_storage_keys: Vec<ContractStorageKeys> = contract_addresses
        .iter()
        .map(|address| ContractStorageKeys {
            contract_address: *address.key(),
            storage_keys: storage_by_address.remove(address).unwrap_or_default(),
        })
        .collect();

    (class_hashes, contract_addresses, contract_storage_keys)
}
