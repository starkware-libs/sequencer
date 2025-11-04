use std::collections::HashMap;

use starknet_api::core::{ClassHash, ContractAddress};
use starknet_api::hash::HashOutput;
use starknet_patricia::impl_from_hex_for_felt_wrapper;
use starknet_patricia::patricia_merkle_tree::filled_tree::tree::FilledTreeImpl;
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::PreimageMap;
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;
use starknet_types_core::felt::{Felt, FromStrError};

use crate::block_committer::input::StarknetStorageValue;
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;

pub fn fixed_hex_string_no_prefix(felt: &Felt) -> String {
    format!("{felt:064x}")
}

pub fn class_hash_into_node_index(class_hash: &ClassHash) -> NodeIndex {
    NodeIndex::from_leaf_felt(&class_hash.0)
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CompiledClassHash(pub Felt);

impl_from_hex_for_felt_wrapper!(CompiledClassHash);

pub type StorageTrie = FilledTreeImpl<StarknetStorageValue>;
pub type ClassesTrie = FilledTreeImpl<CompiledClassHash>;
pub type ContractsTrie = FilledTreeImpl<ContractState>;
pub type StorageTrieMap = HashMap<ContractAddress, StorageTrie>;

pub struct ContractsTrieProof {
    pub nodes: PreimageMap,
    pub leaves: HashMap<ContractAddress, ContractState>,
}

pub struct StarknetForestProofs {
    pub classes_trie_proof: PreimageMap,
    pub contracts_trie_proof: ContractsTrieProof,
    pub contracts_trie_storage_proofs: HashMap<ContractAddress, PreimageMap>,
}

impl StarknetForestProofs {
    pub(crate) fn extend(&mut self, other: Self) {
        self.classes_trie_proof.extend(other.classes_trie_proof);
        self.contracts_trie_proof.nodes.extend(other.contracts_trie_proof.nodes);
        self.contracts_trie_proof.leaves.extend(other.contracts_trie_proof.leaves);
        for (address, proof) in other.contracts_trie_storage_proofs {
            self.contracts_trie_storage_proofs.entry(address).or_default().extend(proof);
        }
    }
}

pub struct RootHashes {
    pub previous_root_hash: HashOutput,
    pub new_root_hash: HashOutput,
}
