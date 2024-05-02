use crate::felt::Felt;
use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::{
    filled_tree::node::{ClassHash, CompiledClassHash, Nonce},
    types::TreeHeight,
};
use crate::storage::storage_trait::{StorageKey, StorageValue};
use std::collections::HashMap;

#[derive(Debug, PartialEq, Eq, Hash)]
// TODO(Nimrod, 1/6/2024): Swap to starknet-types-core types once implemented.
pub struct ContractAddress(pub Felt);

#[derive(Debug, PartialEq, Eq, Hash)]
// TODO(Nimrod, 1/6/2024): Swap to starknet-types-core types once implemented.
pub struct StarknetStorageKey(pub Felt);

#[allow(dead_code)]
#[derive(Debug, Eq, PartialEq)]
pub struct StarknetStorageValue(pub Felt);

#[allow(dead_code)]
#[derive(Debug, Eq, PartialEq)]
pub struct StateDiff {
    pub address_to_class_hash: HashMap<ContractAddress, ClassHash>,
    pub address_to_nonce: HashMap<ContractAddress, Nonce>,
    pub class_hash_to_compiled_class_hash: HashMap<ClassHash, CompiledClassHash>,
    pub current_contract_state_leaves: HashMap<ContractAddress, ContractState>,
    pub storage_updates:
        HashMap<ContractAddress, HashMap<StarknetStorageKey, StarknetStorageValue>>,
}

#[allow(dead_code)]
#[derive(Debug, Eq, PartialEq)]
pub struct ContractState {
    pub nonce: Nonce,
    pub storage_root_hash: HashOutput,
    pub class_hash: ClassHash,
}

#[allow(dead_code)]
#[derive(Debug, Eq, PartialEq)]
pub struct Input {
    pub storage: HashMap<StorageKey, StorageValue>,
    pub state_diff: StateDiff,
    pub tree_height: TreeHeight,
}
