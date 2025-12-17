use std::collections::HashMap;

use blockifier::state::cached_state::StateMaps;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_api::hash::HashOutput;
use starknet_api::state::StorageKey;
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
    StorageProof as RpcStorageProof,
};
use starknet_types_core::felt::Felt as TypesFelt;

use crate::errors::ProofProviderError;

/// Query parameters for fetching storage proofs from RPC.
pub struct RpcStorageProofsQuery {
    pub class_hashes: Vec<Felt>,
    pub contract_addresses: Vec<ContractAddress>,
    pub contract_storage_keys: Vec<ContractStorageKeys>,
}

/// Complete OS input data built from RPC proofs.
pub struct StorageProofs {
    pub cached_state_input: CachedStateInput,
    pub commitment_infos: CommitmentInfos,
}

/// Converts RPC merkle nodes (hash → MerkleNode mapping) to a PreimageMap.
fn rpc_nodes_to_preimage_map<S: std::hash::BuildHasher>(
    nodes: &indexmap::IndexMap<Felt, starknet_rust_core::types::MerkleNode, S>,
) -> PreimageMap {
    nodes.iter().map(|(hash, node)| (HashOutput(*hash), Preimage::from(node))).collect()
}

/// Wrapper around `JsonRpcClient` for fetching storage proofs.
pub struct RpcStorageProofsProvider(pub JsonRpcClient<HttpTransport>);

impl RpcStorageProofsProvider {
    pub fn new(rpc_url: url::Url) -> Self {
        let transport = HttpTransport::new(rpc_url);
        let client = JsonRpcClient::new(transport);
        Self(client)
    }

    /// Extract query parameters from StateMaps.
    pub fn prepare_query(initial_reads: &StateMaps) -> RpcStorageProofsQuery {
        let class_hashes: Vec<Felt> = initial_reads.class_hashes.values().map(|ch| ch.0).collect();

        let contract_addresses: Vec<ContractAddress> =
            initial_reads.get_contract_addresses().into_iter().collect();

        // Storage keys grouped by contract address.
        let mut storage_by_address: HashMap<ContractAddress, Vec<Felt>> = HashMap::new();
        for (address, key) in initial_reads.storage.keys() {
            storage_by_address.entry(*address).or_default().push(*key.0.key());
        }

        let contract_storage_keys: Vec<ContractStorageKeys> = contract_addresses
            .iter()
            .map(|address| ContractStorageKeys {
                contract_address: *address.0.key(),
                storage_keys: storage_by_address.get(address).cloned().unwrap_or_default(),
            })
            .collect();

        RpcStorageProofsQuery { class_hashes, contract_addresses, contract_storage_keys }
    }

    /// Fetch storage proofs from RPC.
    pub async fn fetch_proofs(
        &self,
        block_number: BlockNumber,
        query: &RpcStorageProofsQuery,
    ) -> Result<RpcStorageProof, ProofProviderError> {
        let block_id = ConfirmedBlockId::Number(block_number.0);
        let contract_addresses: Vec<Felt> =
            query.contract_addresses.iter().map(|a| *a.0.key()).collect();

        let storage_proof = self
            .0
            .get_storage_proof(
                block_id,
                &query.class_hashes,
                &contract_addresses,
                &query.contract_storage_keys,
            )
            .await?;

        Ok(storage_proof)
    }

    /// Converts an RPC storage proof response to OS input format.
    pub fn to_storage_proofs(
        rpc_proof: &RpcStorageProof,
        initial_reads: &StateMaps,
        contract_addresses: &[ContractAddress],
    ) -> StorageProofs {
        let cached_state_input =
            Self::build_cached_state_input(rpc_proof, initial_reads, contract_addresses);
        let commitment_infos = Self::build_commitment_infos(rpc_proof, contract_addresses);

        StorageProofs { cached_state_input, commitment_infos }
    }

    fn build_cached_state_input(
        rpc_proof: &RpcStorageProof,
        initial_reads: &StateMaps,
        contract_addresses: &[ContractAddress],
    ) -> CachedStateInput {
        let (address_to_class_hash, address_to_nonce) = rpc_proof
            .contracts_proof
            .contract_leaves_data
            .iter()
            .zip(contract_addresses)
            .map(|(leaf, addr)| ((*addr, ClassHash(leaf.class_hash)), (*addr, Nonce(leaf.nonce))))
            .unzip();

        let storage = initial_reads.storage.iter().fold(
            HashMap::<ContractAddress, HashMap<StorageKey, TypesFelt>>::new(),
            |mut acc, ((addr, key), val)| {
                acc.entry(*addr).or_default().insert(*key, *val);
                acc
            },
        );

        CachedStateInput {
            storage,
            address_to_class_hash,
            address_to_nonce,
            class_hash_to_compiled_class_hash: initial_reads.compiled_class_hashes.clone(),
        }
    }

    fn build_commitment_infos(
        rpc_proof: &RpcStorageProof,
        contract_addresses: &[ContractAddress],
    ) -> CommitmentInfos {
        let contracts_tree_root = HashOutput(rpc_proof.global_roots.contracts_tree_root);
        let classes_tree_root = HashOutput(rpc_proof.global_roots.classes_tree_root);

        let contracts_trie_commitment_info = CommitmentInfo {
            previous_root: contracts_tree_root,
            updated_root: contracts_tree_root,
            tree_height: SubTreeHeight::ACTUAL_HEIGHT,
            commitment_facts: flatten_preimages(&rpc_nodes_to_preimage_map(
                &rpc_proof.contracts_proof.nodes,
            )),
        };

        let classes_trie_commitment_info = CommitmentInfo {
            previous_root: classes_tree_root,
            updated_root: classes_tree_root,
            tree_height: SubTreeHeight::ACTUAL_HEIGHT,
            commitment_facts: flatten_preimages(&rpc_nodes_to_preimage_map(
                &rpc_proof.classes_proof,
            )),
        };

        let storage_tries_commitment_infos =
            Self::build_storage_commitment_infos(rpc_proof, contract_addresses);

        CommitmentInfos {
            contracts_trie_commitment_info,
            classes_trie_commitment_info,
            storage_tries_commitment_infos,
        }
    }

    /// Builds commitment info for each contract's storage trie.
    ///
    /// Each contract has its own storage trie with a separate root. This function:
    /// 1. Zips together: contract leaf data, addresses, and storage proofs (all in matching order)
    /// 2. For each contract that has a storage_root (via `filter_map`):
    ///    - Extracts the storage_root from the leaf data
    ///    - Converts the RPC storage proof nodes to commitment facts
    ///    - Creates a CommitmentInfo for that contract's storage trie
    ///
    /// The `filter_map` skips contracts where `storage_root` is `None` (contracts with no storage).
    fn build_storage_commitment_infos(
        rpc_proof: &RpcStorageProof,
        contract_addresses: &[ContractAddress],
    ) -> HashMap<ContractAddress, CommitmentInfo> {
        rpc_proof
            .contracts_proof
            .contract_leaves_data
            .iter()
            .zip(contract_addresses)
            .zip(&rpc_proof.contracts_storage_proofs)
            .filter_map(|((leaf, addr), storage_proof)| {
                // `?` returns None if storage_root is None, causing filter_map to skip this entry.
                let storage_root = HashOutput(leaf.storage_root?);
                Some((
                    *addr,
                    CommitmentInfo {
                        previous_root: storage_root,
                        updated_root: storage_root,
                        tree_height: SubTreeHeight::ACTUAL_HEIGHT,
                        commitment_facts: flatten_preimages(&rpc_nodes_to_preimage_map(
                            storage_proof,
                        )),
                    },
                ))
            })
            .collect()
    }
}
