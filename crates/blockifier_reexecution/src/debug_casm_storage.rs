use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use starknet_api::core::CompiledClassHash;

static ORIGINAL_CASMS: LazyLock<Mutex<HashMap<CompiledClassHash, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Store the original CASM JSON for a given compiled_class_hash.
pub fn store_original_casm(compiled_class_hash: CompiledClassHash, json: String) {
    ORIGINAL_CASMS.lock().unwrap().insert(compiled_class_hash, json);
}

/// Retrieve the original CASM JSON for a given compiled_class_hash.
pub fn get_original_casm(compiled_class_hash: CompiledClassHash) -> Option<String> {
    ORIGINAL_CASMS.lock().unwrap().get(&compiled_class_hash).cloned()
}
