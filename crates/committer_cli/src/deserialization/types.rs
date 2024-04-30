use committer::felt::Felt;
use committer::hash::hash_trait::HashOutput;
use committer::patricia_merkle_tree::filled_tree::node::{ClassHash, CompiledClassHash, Nonce};
use committer::patricia_merkle_tree::types::TreeHeight;
use committer::storage::storage_trait::{StorageKey, StorageValue};
use serde::Deserialize;
use std::collections::HashMap;

type RawFelt = [u8; 32];

#[derive(Debug, PartialEq, Eq, Hash)]
// TODO(Nimrod, 1/6/2024): Swap to starknet-types-core types once implemented.
pub(crate) struct ContractAddress(pub Felt);

#[derive(Debug, PartialEq, Eq, Hash)]
// TODO(Nimrod, 1/6/2024): Swap to starknet-types-core types once implemented.
pub(crate) struct StarknetStorageKey(pub Felt);

#[allow(dead_code)]
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct StarknetStorageValue(pub Felt);

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
/// Input to the committer.
pub(crate) struct RawInput {
    /// Storage. Will be casted to HashMap<vec<u8>, Vec<u8>> to simulate DB access.
    pub storage: Vec<RawStorageEntry>,
    /// All relevant information for the state diff commitment.
    pub state_diff: RawStateDiff,
    /// The height of the patricia tree.
    // TODO(Nimrod,20/4/2024): Strong assumption - all trees have same height. How can I get
    // rid of it?
    pub tree_height: u8,
}

#[allow(dead_code)]
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct Input {
    pub storage: HashMap<StorageKey, StorageValue>,
    pub state_diff: StateDiff,
    pub tree_height: TreeHeight,
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
/// Fact storage entry.
pub(crate) struct RawStorageEntry {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
pub(crate) struct RawFeltMapEntry {
    pub key: RawFelt,
    pub value: RawFelt,
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
/// Represents storage updates. Later will be casted to HashMap<Felt, HashMap<Felt,Felt>> entry.
pub(crate) struct RawStorageUpdates {
    pub address: RawFelt,
    pub storage_updates: Vec<RawFeltMapEntry>,
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
/// Represents current state leaf at the contract state tree. Later will be casted to
/// HashMap<Felt, (nonce, class_hash, storage_root_hash)> entry.
pub(crate) struct RawContractStateLeaf {
    pub address: RawFelt,
    pub nonce: RawFelt,
    pub storage_root_hash: RawFelt,
    pub class_hash: RawFelt,
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
/// Represents state diff.
pub(crate) struct RawStateDiff {
    /// Will be casted to HashMap<Felt, Felt>.
    pub address_to_class_hash: Vec<RawFeltMapEntry>,
    /// Will be casted to HashMap<Felt, Felt>.
    pub address_to_nonce: Vec<RawFeltMapEntry>,
    /// Will be casted to HashMap<Felt, Felt>.
    pub class_hash_to_compiled_class_hash: Vec<RawFeltMapEntry>,
    /// Will be casted to HashMap<Felt, HashMap<Felt, Felt>>.
    pub storage_updates: Vec<RawStorageUpdates>,
    /// Will be casted to HashMap<Felt, ContractState>.
    pub current_contract_state_leaves: Vec<RawContractStateLeaf>,
}

#[allow(dead_code)]
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct StateDiff {
    pub address_to_class_hash: HashMap<ContractAddress, ClassHash>,
    pub address_to_nonce: HashMap<ContractAddress, Nonce>,
    pub class_hash_to_compiled_class_hash: HashMap<ClassHash, CompiledClassHash>,
    pub current_contract_state_leaves: HashMap<ContractAddress, ContractState>,
    pub storage_updates:
        HashMap<ContractAddress, HashMap<StarknetStorageKey, StarknetStorageValue>>,
}

#[allow(dead_code)]
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct ContractState {
    pub nonce: Nonce,
    pub storage_root_hash: HashOutput,
    pub class_hash: ClassHash,
}
