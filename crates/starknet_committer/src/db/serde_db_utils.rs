#[cfg(feature = "os_input")]
use std::collections::HashMap;

#[cfg(feature = "os_input")]
use blake2::Blake2s256;
#[cfg(feature = "os_input")]
use digest::Digest;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
#[cfg(feature = "os_input")]
use starknet_api::core::{ClassHash, ContractAddress};
#[cfg(feature = "os_input")]
use starknet_api::state::StorageKey;
use starknet_patricia_storage::storage_trait::DbValue;
use starknet_types_core::felt::Felt;

#[cfg(feature = "os_input")]
use crate::block_committer::input::StarknetStorageKey;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Hash, Serialize)]
pub struct DbBlockNumber(pub BlockNumber);

impl DbBlockNumber {
    pub fn serialize(&self) -> [u8; 8] {
        self.0.0.to_be_bytes()
    }

    pub fn deserialize(value: [u8; 8]) -> Self {
        Self(BlockNumber(u64::from_be_bytes(value)))
    }
}

pub fn serialize_felt_no_packing(felt: Felt) -> DbValue {
    DbValue(felt.to_bytes_be().to_vec())
}

pub fn deserialize_felt_no_packing(value: &DbValue) -> Felt {
    Felt::from_bytes_be_slice(&value.0)
}

/// Wire layout for [`accessed_keys_digest`]. Field order and sorting are part of the digest.
#[cfg(feature = "os_input")]
#[derive(Serialize, Deserialize)]
struct AccessedKeysCanonical {
    class_hashes: Vec<ClassHash>,
    /// Contract trie reads (e.g. class / nonce), sorted.
    contract_addresses: Vec<ContractAddress>,
    /// Storage-slot reads: sorted by contract address; each key list sorted by underlying felt.
    contract_storage: Vec<(ContractAddress, Vec<StorageKey>)>,
}

/// BLAKE2s-256 digest of canonical serialized accessed read keys (replay fingerprint).
#[cfg(feature = "os_input")]
pub fn accessed_keys_digest(
    class_hashes: &[ClassHash],
    contract_addresses: &[ContractAddress],
    contract_storage_keys: &HashMap<ContractAddress, Vec<StarknetStorageKey>>,
) -> [u8; 32] {
    let mut class_hashes: Vec<ClassHash> = class_hashes.iter().copied().collect();
    class_hashes.sort();

    let mut contract_addresses: Vec<ContractAddress> = contract_addresses.iter().copied().collect();
    contract_addresses.sort();

    let mut contract_storage: Vec<(ContractAddress, Vec<StorageKey>)> = contract_storage_keys
        .iter()
        .map(|(addr, keys)| {
            let mut keys: Vec<StorageKey> = keys.iter().map(|k| k.0).collect();
            keys.sort_by_key(|k| Felt::from(*k));
            (*addr, keys)
        })
        .collect();
    contract_storage.sort_by_key(|(addr, _)| *addr);

    let canonical = AccessedKeysCanonical { class_hashes, contract_addresses, contract_storage };
    let bytes = bincode::serialize(&canonical).expect("accessed keys bincode serialization");

    let hash = Blake2s256::digest(&bytes);
    hash.into()
}
