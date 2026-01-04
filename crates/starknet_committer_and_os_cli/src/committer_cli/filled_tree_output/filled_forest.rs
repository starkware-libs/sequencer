use serde::Serialize;
use starknet_committer::db::facts_db::db::FactsDb;
use starknet_committer::db::forest_trait::ForestWriter;
use starknet_committer::forest::filled_forest::FilledForest;
use starknet_patricia_storage::errors::SerializationResult;
use starknet_patricia_storage::map_storage::MapStorage;

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
    pub async fn forest_to_output(&self) -> SerializationResult<Output> {
        // Create an empty storage for the new facts.
        let mut output_facts_db = FactsDb::new(MapStorage::default());
        output_facts_db.write(&self.0).await?;
        let contract_storage_root_hash = self.0.get_contract_root_hash().0;
        let compiled_class_root_hash = self.0.get_compiled_class_root_hash().0;
        Ok(Output {
            storage: output_facts_db.storage,
            contract_storage_root_hash: contract_storage_root_hash.to_hex_string(),
            compiled_class_root_hash: compiled_class_root_hash.to_hex_string(),
        })
    }
}
