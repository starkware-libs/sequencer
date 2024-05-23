use serde::Deserialize;

type RawFelt = [u8; 32];

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
/// Input to the committer.
pub(crate) struct RawInput {
    /// Storage. Will be casted to HashMap<vec<u8>, Vec<u8>> to simulate DB access.
    pub storage: Vec<RawStorageEntry>,
    pub state_diff: RawStateDiff,
    pub tree_heights: u8,
    pub contracts_trie_root_hash: RawFelt,
    pub classes_trie_root_hash: RawFelt,
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
    pub current_contracts_trie_leaves: Vec<RawContractStateLeaf>,
}
