use std::collections::HashMap;

use serde::Serialize;
use starknet_committer::forest::filled_forest::FilledForest;
use starknet_patricia_storage::map_storage::BorrowedMapStorage;

pub struct SerializedForest(pub FilledForest);

#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub struct Output<'a> {
    // New fact storage.
    storage: BorrowedMapStorage<'a>,
    // TODO(Amos, 1/8/2024): Rename to `contracts_trie_root_hash` & `classes_trie_root_hash`.
    // New contract storage root.
    pub contract_storage_root_hash: String,
    // New compiled class root.
    pub compiled_class_root_hash: String,
}

impl SerializedForest {
<<<<<<< HEAD
    pub fn forest_to_output(&self) -> Output {
        // Create an empty storage for the new facts.
        let mut storage = HashMap::new();
||||||| 01792faa8
    pub fn forest_to_output(&self) -> Output {
        let mut storage = MapStorage::default();
=======
    pub fn forest_to_output<'a>(&self, mut storage: BorrowedMapStorage<'a>) -> Output<'a> {
>>>>>>> origin/main-v0.14.1
        self.0.write_to_storage(&mut storage);
        let contract_storage_root_hash = self.0.get_contract_root_hash().0;
        let compiled_class_root_hash = self.0.get_compiled_class_root_hash().0;
        Output {
            storage,
            contract_storage_root_hash: contract_storage_root_hash.to_hex_string(),
            compiled_class_root_hash: compiled_class_root_hash.to_hex_string(),
        }
    }
}
