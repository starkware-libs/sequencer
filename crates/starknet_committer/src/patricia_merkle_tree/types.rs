use std::collections::HashMap;

use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress};
use starknet_patricia::patricia_merkle_tree::filled_tree::tree::FilledTreeImpl;
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;
use starknet_types_core::felt::Felt;

use crate::block_committer::input::StarknetStorageValue;
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;

pub fn fixed_hex_string_no_prefix(felt: &Felt) -> String {
    felt.to_fixed_hex_string().strip_prefix("0x").unwrap_or("0").to_string()
}

pub fn from_class_hash_for_node_index(class_hash: &ClassHash) -> NodeIndex {
    NodeIndex::from_leaf_felt(&class_hash.0)
}

pub type StorageTrie = FilledTreeImpl<StarknetStorageValue>;
pub type ClassesTrie = FilledTreeImpl<CompiledClassHash>;
pub type ContractsTrie = FilledTreeImpl<ContractState>;
pub type StorageTrieMap = HashMap<ContractAddress, StorageTrie>;
