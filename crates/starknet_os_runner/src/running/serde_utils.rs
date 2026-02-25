use blockifier::state::cached_state::StateMaps;
use serde::Deserialize;
use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

// TODO(Aviv): Delete all these structs when similar structs will be available in starknet rust or
// other libraries.
#[derive(Deserialize)]
struct StorageEntry {
    contract_address: ContractAddress,
    key: StorageKey,
    value: Felt,
}

#[derive(Deserialize)]
struct NonceEntry {
    contract_address: ContractAddress,
    nonce: Nonce,
}

#[derive(Deserialize)]
struct ClassHashEntry {
    contract_address: ContractAddress,
    class_hash: ClassHash,
}

#[derive(Deserialize)]
struct DeclaredEntry {
    class_hash: ClassHash,
    is_declared: bool,
}

/// Pathfinder v0.10 `initial_reads` response shape.
#[derive(Deserialize, Default)]
struct RpcInitialReads {
    #[serde(default)]
    storage: Vec<StorageEntry>,
    #[serde(default)]
    nonces: Vec<NonceEntry>,
    #[serde(default)]
    class_hashes: Vec<ClassHashEntry>,
    #[serde(default)]
    declared_contracts: Vec<DeclaredEntry>,
}

impl From<RpcInitialReads> for StateMaps {
    fn from(r: RpcInitialReads) -> Self {
        let mut m = StateMaps::default();
        m.storage.extend(r.storage.into_iter().map(|e| ((e.contract_address, e.key), e.value)));
        m.nonces.extend(r.nonces.into_iter().map(|e| (e.contract_address, e.nonce)));
        m.class_hashes
            .extend(r.class_hashes.into_iter().map(|e| (e.contract_address, e.class_hash)));
        m.declared_contracts
            .extend(r.declared_contracts.into_iter().map(|e| (e.class_hash, e.is_declared)));
        m
    }
}

/// Deserializes pathfinder's v0.10 `initial_reads` JSON into blockifier `StateMaps`.
#[allow(dead_code)]
pub(crate) fn deserialize_rpc_initial_reads(value: serde_json::Value) -> Result<StateMaps, String> {
    serde_json::from_value::<RpcInitialReads>(value).map(Into::into).map_err(|e| e.to_string())
}
