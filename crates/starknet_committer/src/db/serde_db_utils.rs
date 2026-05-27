#[cfg(feature = "os_input")]
use blake2::Blake2s256;
#[cfg(feature = "os_input")]
use digest::Digest;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
#[cfg(feature = "os_input")]
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;
use starknet_patricia_storage::storage_trait::DbValue;
use starknet_types_core::felt::Felt;

#[cfg(feature = "os_input")]
use crate::patricia_merkle_tree::tree::SortedLeavesRequest;

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

/// BLAKE2s-256 digest over a deterministic encoding of [`SortedLeavesRequest`].
///
/// The hashed payload is a concatenation length-prefixed big-endian 32-byte
/// [`NodeIndex`] values (`idx.0.to_be_bytes()`), in this order:
///
/// 1. Classes trie — `len(classes_sorted)` then each class leaf index (already sorted).
/// 2. Contracts trie — `len(contracts_sorted)` then each contract leaf index (already sorted).
/// 3. Storage tries — `len(storage_sorted)` then, for each contract index in ascending order: the
///    contract index, `len(storage slot indices)` for that contract, then each storage slot index
///    (already sorted within the contract).
#[cfg(feature = "os_input")]
pub fn accessed_keys_digest(sorted: &SortedLeavesRequest<'_>) -> [u8; 32] {
    let mut payload = Vec::new();

    let class = sorted.class_sorted.get_indices();
    payload.extend_from_slice(&encode_usize(class.len()));
    for idx in class {
        payload.extend_from_slice(&idx.0.to_be_bytes());
    }

    let contract = sorted.contract_sorted.get_indices();
    payload.extend_from_slice(&encode_usize(contract.len()));
    for idx in contract {
        payload.extend_from_slice(&idx.0.to_be_bytes());
    }

    let mut contract_indices: Vec<NodeIndex> = sorted.storage_sorted.keys().copied().collect();
    contract_indices.sort_unstable();

    payload.extend_from_slice(&encode_usize(contract_indices.len()));
    for contract_idx in contract_indices {
        let sorted_slots = sorted.storage_sorted.get(&contract_idx).unwrap();
        payload.extend_from_slice(&contract_idx.0.to_be_bytes());
        let slot_indices = sorted_slots.get_indices();
        payload.extend_from_slice(&encode_usize(slot_indices.len()));
        for slot in slot_indices {
            payload.extend_from_slice(&slot.0.to_be_bytes());
        }
    }

    Blake2s256::digest(&payload).into()
}

#[cfg(feature = "os_input")]
fn encode_usize(n: usize) -> [u8; 8] {
    u64::try_from(n).expect("accessed leaf count exceeds u64::MAX").to_be_bytes()
}
