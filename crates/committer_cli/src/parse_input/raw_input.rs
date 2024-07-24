use serde::Deserialize;

type RawFelt = [u8; 32];

#[derive(Deserialize, Debug)]
/// Input to the committer.
pub(crate) struct RawInput {
    /// Storage. Will be casted to HashMap<vec<u8>, Vec<u8>> to simulate DB access.
    pub storage: Vec<RawStorageEntry>,
    pub state_diff: RawStateDiff,
    pub contracts_trie_root_hash: RawFelt,
    pub classes_trie_root_hash: RawFelt,
    pub trivial_updates_config: bool,
}

#[derive(Deserialize, Debug)]
/// Fact storage entry.
pub(crate) struct RawStorageEntry {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}

#[derive(Deserialize, Debug)]
pub(crate) struct RawFeltMapEntry {
    pub key: RawFelt,
    pub value: RawFelt,
}

#[derive(Deserialize, Debug)]
/// Represents storage updates. Later will be casted to HashMap<Felt, HashMap<Felt,Felt>> entry.
pub(crate) struct RawStorageUpdates {
    pub address: RawFelt,
    pub storage_updates: Vec<RawFeltMapEntry>,
}

#[derive(Deserialize, Debug)]
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
}
