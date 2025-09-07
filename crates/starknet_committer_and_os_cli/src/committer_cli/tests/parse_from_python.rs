use std::collections::HashMap;

use ethnum::U256;
use serde::{Deserialize, Deserializer};
use starknet_committer::block_committer::input::StarknetStorageValue;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::LeafModifications;
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;
use starknet_patricia_storage::map_storage::MapStorage;
use starknet_patricia_storage::storage_trait::{DbKey, DbValue};
use starknet_types_core::felt::Felt;

use crate::committer_cli::parse_input::cast::add_unique;
use crate::committer_cli::parse_input::raw_input::RawStorageEntry;

pub struct TreeFlowInput {
    pub leaf_modifications: LeafModifications<StarknetStorageValue>,
    pub storage: MapStorage,
    pub root_hash: HashOutput,
}

impl<'de> Deserialize<'de> for TreeFlowInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let map = HashMap::deserialize(deserializer)?;
        Ok(parse_input_single_storage_tree_flow_test(&map))
    }
}

#[allow(clippy::unwrap_used)]
/// Parse input for single storage tree flow test.
/// Returns the leaf modifications, fetched nodes (in storage) and the root hash.
pub fn parse_input_single_storage_tree_flow_test(input: &HashMap<String, String>) -> TreeFlowInput {
    // Fetch leaf_modifications.
    let leaf_modifications_json = input.get("leaf_modifications").unwrap();
    let leaf_modifications_map =
        serde_json::from_str::<HashMap<&str, &str>>(leaf_modifications_json).unwrap();
    let leaf_modifications = leaf_modifications_map
        .iter()
        .map(|(k, v)| {
            (
                NodeIndex::new(U256::from_str_hex(k).unwrap()),
                StarknetStorageValue(Felt::from_hex(v).unwrap()),
            )
        })
        .collect();

    // Fetch storage.
    let raw_storage =
        serde_json::from_str::<Vec<RawStorageEntry>>(input.get("storage").unwrap()).unwrap();

    let mut storage = HashMap::new();
    for entry in raw_storage {
        add_unique(&mut storage, "storage", DbKey(entry.key), DbValue(entry.value)).unwrap();
    }

    // Fetch root_hash.
    let root_hash = HashOutput(Felt::from_hex(input.get("root_hash").unwrap()).unwrap());

    TreeFlowInput { leaf_modifications, storage, root_hash }
}
