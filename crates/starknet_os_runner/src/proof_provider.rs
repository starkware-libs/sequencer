//! RPC proof provider for fetching proofs and building OS input.

use std::collections::HashMap;

use blockifier::state::cached_state::StateMaps;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_api::hash::HashOutput;
use starknet_api::state::StorageKey;
use starknet_committer::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use starknet_committer::patricia_merkle_tree::types::{ContractsTrieProof, StarknetForestProofs};
use starknet_os::io::os_input::{CachedStateInput, CommitmentInfo, CommitmentInfos};
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::{
    flatten_preimages,
    Preimage,
    PreimageMap,
};
use starknet_patricia::patricia_merkle_tree::types::SubTreeHeight;
use starknet_rust::providers::jsonrpc::HttpTransport;
use starknet_rust::providers::{JsonRpcClient, Provider};
use starknet_rust_core::types::{
    ConfirmedBlockId,
    ContractStorageKeys,
    Felt,
    MerkleNode,
    StorageProof,
};

use crate::errors::ProofProviderError;

/// Query parameters extracted from StateMaps for the RPC call.
pub struct RpcProofQueryParams {
    pub class_hashes: Vec<Felt>,
    pub contract_addresses: Vec<Felt>,
    pub contract_storage_keys: Vec<ContractStorageKeys>,
}

/// Complete OS input built from RPC proofs.
pub struct OsInputFromProofs {
    pub cached_state_input: CachedStateInput,
    pub commitment_infos: CommitmentInfos,
}

/// Wrapper around `JsonRpcClient` for fetching proofs and building OS input.
pub struct RpcProofProvider(pub JsonRpcClient<HttpTransport>);

impl RpcProofProvider {
    /// Create a new provider wrapping the given client.
    pub fn new(client: JsonRpcClient<HttpTransport>) -> Self {
        Self(client)
    }

    /// Extract query parameters from StateMaps.
    pub fn prepare_query(initial_reads: StateMaps) -> RpcProofQueryParams {
        // Class hashes.
        let class_hashes: Vec<Felt> = initial_reads.class_hashes.values().map(|ch| ch.0).collect();
        // TODO: should we include the class hashes from compiled_class_hashes?

        // Contract addresses: all unique addresses from nonces, class_hashes, and storage.
        let contract_addresses_typed: Vec<ContractAddress> =
            initial_reads.get_contract_addresses().into_iter().collect();

        let contract_addresses: Vec<Felt> =
            contract_addresses_typed.iter().map(|addr| *addr.0.key()).collect();

        // Storage keys grouped by contract address.
        let mut storage_by_address: HashMap<ContractAddress, Vec<Felt>> = HashMap::new();
        for (address, key) in initial_reads.storage.keys() {
            storage_by_address.entry(*address).or_default().push(*key.0.key());
        }

        let contract_storage_keys: Vec<ContractStorageKeys> = contract_addresses_typed
            .iter()
            .map(|address| ContractStorageKeys {
                contract_address: *address.0.key(),
                storage_keys: storage_by_address.remove(address).unwrap_or_default(),
            })
            .collect();

        RpcProofQueryParams { class_hashes, contract_addresses, contract_storage_keys }
    }

    /// Fetch StorageProof from RPC.
    pub async fn fetch_storage_proof(
        &self,
        block_number: BlockNumber,
        query: &RpcProofQueryParams,
    ) -> Result<StorageProof, ProofProviderError> {
        let block_id = ConfirmedBlockId::Number(block_number.0);

        let storage_proof = self
            .0
            .get_storage_proof(
                block_id,
                &query.class_hashes,
                &query.contract_addresses,
                &query.contract_storage_keys,
            )
            .await?;

        Ok(storage_proof)
    }

    /// Convert StorageProof to OsInputFromProofs.
    ///
    /// Assumes no state changes occurred (previous state == new state).
    pub fn to_os_input(
        proof: StorageProof,
        query: &RpcProofQueryParams,
        state_maps: &StateMaps,
    ) -> OsInputFromProofs {
        let contract_addresses = &query.contract_addresses;

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

        // Build forest proofs.
        let contracts_trie_storage_proofs: HashMap<ContractAddress, PreimageMap> =
            contract_addresses
                .iter()
                .zip(proof.contracts_storage_proofs.iter())
                .map(|(address_felt, nodes)| {
                    let address =
                        ContractAddress::try_from(*address_felt).expect("Invalid contract address");
                    (address, index_map_to_preimage_map(nodes))
                })
                .collect();

        let leaves: HashMap<ContractAddress, ContractState> = contract_addresses
            .iter()
            .zip(proof.contracts_proof.contract_leaves_data.iter())
            .map(|(address_felt, leaf_data)| {
                let address =
                    ContractAddress::try_from(*address_felt).expect("Invalid contract address");
                let contract_state = ContractState {
                    nonce: Nonce(leaf_data.nonce),
                    class_hash: ClassHash(leaf_data.class_hash),
                    storage_root_hash: HashOutput(leaf_data.storage_root.unwrap_or(Felt::ZERO)),
                };
                (address, contract_state)
            })
            .collect();

        let forest_proofs = StarknetForestProofs {
            classes_trie_proof: index_map_to_preimage_map(&proof.classes_proof),
            contracts_trie_proof: ContractsTrieProof {
                nodes: index_map_to_preimage_map(&proof.contracts_proof.nodes),
                leaves: leaves.clone(),
            },
            contracts_trie_storage_proofs,
        };

        let contracts_trie_root = HashOutput(proof.global_roots.contracts_tree_root);
        let classes_trie_root = HashOutput(proof.global_roots.classes_tree_root);

        // Build CachedStateInput.
        let cached_state_input = build_cached_state_input(&leaves, state_maps);

        // Build CommitmentInfos.
        let commitment_infos =
            build_commitment_infos(&forest_proofs, &leaves, contracts_trie_root, classes_trie_root);

        OsInputFromProofs { cached_state_input, commitment_infos }
    }
}

/// Convert an IndexMap of Felt -> MerkleNode to PreimageMap.
fn index_map_to_preimage_map<S>(nodes: &indexmap::IndexMap<Felt, MerkleNode, S>) -> PreimageMap {
    nodes.iter().map(|(hash, node)| (HashOutput(*hash), Preimage::from(node))).collect()
}

/// Build CachedStateInput from contract leaves and state maps.
fn build_cached_state_input(
    leaves: &HashMap<ContractAddress, ContractState>,
    state_maps: &StateMaps,
) -> CachedStateInput {
    let mut address_to_class_hash = HashMap::new();
    let mut address_to_nonce = HashMap::new();

    for (address, contract_state) in leaves {
        address_to_class_hash.insert(*address, contract_state.class_hash);
        address_to_nonce.insert(*address, contract_state.nonce);
    }

    // Build storage from StateMaps - group by contract address.
    let mut storage: HashMap<ContractAddress, HashMap<StorageKey, Felt>> = HashMap::new();
    for ((address, key), value) in &state_maps.storage {
        storage.entry(*address).or_default().insert(*key, *value);
    }

    CachedStateInput {
        storage,
        address_to_class_hash,
        address_to_nonce,
        class_hash_to_compiled_class_hash: state_maps.compiled_class_hashes.clone(),
    }
}

/// Build CommitmentInfos from forest proofs.
/// Assumes no state changes (previous_root == updated_root).
fn build_commitment_infos(
    forest_proofs: &StarknetForestProofs,
    leaves: &HashMap<ContractAddress, ContractState>,
    contracts_trie_root: HashOutput,
    classes_trie_root: HashOutput,
) -> CommitmentInfos {
    let contracts_trie = CommitmentInfo {
        previous_root: contracts_trie_root,
        updated_root: contracts_trie_root,
        tree_height: SubTreeHeight::ACTUAL_HEIGHT,
        commitment_facts: flatten_preimages(&forest_proofs.contracts_trie_proof.nodes),
    };

    let classes_trie = CommitmentInfo {
        previous_root: classes_trie_root,
        updated_root: classes_trie_root,
        tree_height: SubTreeHeight::ACTUAL_HEIGHT,
        commitment_facts: flatten_preimages(&forest_proofs.classes_trie_proof),
    };

    let storage_tries = leaves
        .iter()
        .map(|(address, contract_state)| {
            let storage_proof = forest_proofs
                .contracts_trie_storage_proofs
                .get(address)
                .map(|p| flatten_preimages(p))
                .unwrap_or_default();

            let commitment_info = CommitmentInfo {
                previous_root: contract_state.storage_root_hash,
                updated_root: contract_state.storage_root_hash,
                tree_height: SubTreeHeight::ACTUAL_HEIGHT,
                commitment_facts: storage_proof,
            };
            (*address, commitment_info)
        })
        .collect();

    CommitmentInfos { contracts_trie, classes_trie, storage_tries }
}
