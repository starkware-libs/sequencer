use serde::Serialize;
use starknet_committer::forest::filled_forest::FilledForest;
use starknet_patricia::storage::map_storage::MapStorage;

pub struct SerializedForest(pub FilledForest);

#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub struct Output {
    // New fact storage.
    storage: MapStorage,
    // TODO(Amos, 1/8/2024): Rename to `contracts_trie_root_hash` & `classes_trie_root_hash`.
    // New contract storage root.
    pub contract_storage_root_hash: String,
    // New compiled class root.
    pub compiled_class_root_hash: String,
}

impl SerializedForest {
    pub fn forest_to_output(&self) -> Output {
        let mut storage = MapStorage::default();
        self.0.write_to_storage(&mut storage);
        let contract_storage_root_hash = self.0.get_contract_root_hash().0;
        let compiled_class_root_hash = self.0.get_compiled_class_root_hash().0;
        Output {
            storage,
            contract_storage_root_hash: contract_storage_root_hash.to_hex(),
            compiled_class_root_hash: compiled_class_root_hash.to_hex(),
        }
    }
}
