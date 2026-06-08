use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use starknet_api::core::{ClassHash, ContractAddress};
use starknet_api::hash::HashOutput;
use starknet_patricia::impl_from_hex_for_felt_wrapper;
use starknet_patricia::patricia_merkle_tree::filled_tree::tree::FilledTreeImpl;
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::PreimageMap;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::SkeletonLeaf;
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;
use starknet_types_core::felt::{Felt, FromStrError};

use crate::block_committer::input::{try_node_index_into_contract_address, StarknetStorageValue};
use crate::db::db_layout::DbLayout;
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;

pub fn fixed_hex_string_no_prefix(felt: &Felt) -> String {
    format!("{felt:064x}")
}

pub fn class_hash_into_node_index(class_hash: &ClassHash) -> NodeIndex {
    NodeIndex::from_leaf_felt(&class_hash.0)
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CompiledClassHash(pub Felt);

impl From<CompiledClassHash> for SkeletonLeaf {
    fn from(compiled_class_hash: CompiledClassHash) -> Self {
        SkeletonLeaf::from(compiled_class_hash.0)
    }
}

impl AsRef<CompiledClassHash> for CompiledClassHash {
    fn as_ref(&self) -> &CompiledClassHash {
        self
    }
}

impl_from_hex_for_felt_wrapper!(CompiledClassHash);

pub type StorageTrie = FilledTreeImpl<StarknetStorageValue>;
pub type ClassesTrie = FilledTreeImpl<CompiledClassHash>;
pub type ContractsTrie = FilledTreeImpl<ContractState>;
pub type StorageTrieMap = HashMap<ContractAddress, StorageTrie>;

#[derive(Clone, Debug, PartialEq)]
pub struct ContractsTrieProof {
    pub nodes: PreimageMap,
    pub leaves: HashMap<ContractAddress, ContractState>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StarknetForestProofs {
    pub classes_trie_proof: PreimageMap,
    pub contracts_trie_proof: ContractsTrieProof,
    pub contracts_trie_storage_proofs: HashMap<ContractAddress, PreimageMap>,
}

impl StarknetForestProofs {
    pub fn build<Layout>(
        classes_trie_proof: PreimageMap,
        contracts_proof_nodes: PreimageMap,
        contract_leaves: HashMap<NodeIndex, Layout::ContractStateDbLeaf>,
        contracts_trie_storage_proofs: HashMap<ContractAddress, PreimageMap>,
    ) -> Self
    where
        Layout: DbLayout,
        Layout::ContractStateDbLeaf: Into<ContractState>,
    {
        // Convert contract_leaves_data keys from NodeIndex to ContractAddress.
        let contract_leaves_data: HashMap<ContractAddress, ContractState> = contract_leaves
            .into_iter()
            .map(|(idx, contract_state_leaf)| {
                (
                    try_node_index_into_contract_address(&idx).unwrap_or_else(|_| {
                        panic!(
                            "Converting leaf NodeIndex to ContractAddress should succeed; failed \
                             to convert {idx:?}."
                        )
                    }),
                    contract_state_leaf.into(),
                )
            })
            .collect();

        Self {
            classes_trie_proof,
            contracts_trie_proof: ContractsTrieProof {
                nodes: contracts_proof_nodes,
                leaves: contract_leaves_data,
            },
            contracts_trie_storage_proofs,
        }
    }

    pub fn extend(&mut self, other: Self) {
        self.classes_trie_proof.extend(other.classes_trie_proof);
        self.contracts_trie_proof.nodes.extend(other.contracts_trie_proof.nodes);
        self.contracts_trie_proof.leaves.extend(other.contracts_trie_proof.leaves);
        for (address, proof) in other.contracts_trie_storage_proofs {
            self.contracts_trie_storage_proofs.entry(address).or_default().extend(proof);
        }
    }

    pub fn get_nodes_count(&self) -> usize {
        self.classes_trie_proof.len()
            + self.contracts_trie_proof.nodes.len()
            + self.contracts_trie_proof.leaves.len()
            + self
                .contracts_trie_storage_proofs
                .values()
                .fold(0, |count, proofs| count + proofs.len())
    }
}

pub struct RootHashes {
    pub previous_root_hash: HashOutput,
    pub new_root_hash: HashOutput,
}
