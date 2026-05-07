use std::collections::HashMap;

use serde::de::Error as DeError;
use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_api::hash::HashOutput;
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::{
    flatten_preimages,
    Preimage,
    PreimageMap,
};
use starknet_patricia_storage::errors::{
    DeserializationError,
    SerializationError,
    SerializationResult,
};
use starknet_patricia_storage::storage_trait::DbValue;
use starknet_types_core::felt::Felt;

use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use crate::patricia_merkle_tree::types::{ContractsTrieProof, StarknetForestProofs};

type RawPreimages = Vec<(HashOutput, Vec<Felt>)>;
type ContractTrieLeaves = Vec<(ContractAddress, (Nonce, HashOutput, ClassHash))>;
type StorageTrieProofs = Vec<(ContractAddress, RawPreimages)>;
type SerializedStarknetForestProofs =
    (RawPreimages, RawPreimages, ContractTrieLeaves, StorageTrieProofs);

impl StarknetForestProofs {
    /// Zstd-compressed bincode payload for the OS-input witnesses.
    ///
    /// The inner bincode payload is a 4-tuple, in order:
    ///
    /// 1. Classes trie inner nodes — `RawPreimages`.
    /// 2. Contract trie inner nodes — `RawPreimages`.
    /// 3. Contract trie leaves — `ContractTrieLeaves`.
    /// 4. Storage tries inner nodes — `StorageTrieProofs`.
    ///
    /// Each `commitment_facts` entry uses the same encoding as [`CommitmentInfo::commitment_facts`]
    /// and OS Patricia hints:
    ///
    /// - Binary node — `[left: Felt, right: Felt]`.
    /// - Edge node — `[length: Felt, path: Felt, bottom: Felt]`.
    pub fn serialize(&self) -> SerializationResult<DbValue> {
        let classes: RawPreimages =
            flatten_preimages(&self.classes_trie_proof).into_iter().collect();
        let contract_nodes: RawPreimages =
            flatten_preimages(&self.contracts_trie_proof.nodes).into_iter().collect();
        let contract_leaves: ContractTrieLeaves = self
            .contracts_trie_proof
            .leaves
            .iter()
            .map(|(addr, contract_state)| {
                (
                    *addr,
                    (
                        contract_state.nonce,
                        contract_state.storage_root_hash,
                        contract_state.class_hash,
                    ),
                )
            })
            .collect();

        let storage: StorageTrieProofs = self
            .contracts_trie_storage_proofs
            .iter()
            .map(|(addr, preimage_map)| {
                (*addr, flatten_preimages(preimage_map).into_iter().collect())
            })
            .collect();

        let bincode_payload =
            bincode::serialize(&(classes, contract_nodes, contract_leaves, storage))
                .map_err(bincode_ser_err)?;
        let compressed =
            zstd::encode_all(bincode_payload.as_slice(), zstd::DEFAULT_COMPRESSION_LEVEL)
                .map_err(SerializationError::IOSerialize)?;
        Ok(DbValue(compressed))
    }

    pub fn deserialize(value: &DbValue) -> Result<Self, DeserializationError> {
        let bincode_payload = zstd::decode_all(value.0.as_slice()).map_err(|error| {
            DeserializationError::ValueError(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                error.to_string(),
            )))
        })?;
        let (classes, contract_nodes, contract_leaves, storage): SerializedStarknetForestProofs =
            bincode::deserialize(&bincode_payload).map_err(|error| {
                DeserializationError::ValueError(Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    error.to_string(),
                )))
            })?;

        let classes_trie_proof = preimage_map_from_commitment_facts(classes)?;
        let contracts_trie_proof = ContractsTrieProof {
            nodes: preimage_map_from_commitment_facts(contract_nodes)?,
            leaves: contract_leaves.into_iter().try_fold(
                HashMap::new(),
                |mut leaves, (addr, (nonce, storage_root_hash, class_hash))| {
                    if leaves
                        .insert(addr, ContractState { nonce, storage_root_hash, class_hash })
                        .is_some()
                    {
                        return Err(DeserializationError::KeyDuplicate(format!(
                            "duplicate contracts trie leaf {addr:?}"
                        )));
                    }
                    Ok(leaves)
                },
            )?,
        };

        let contracts_trie_storage_proofs =
            storage.into_iter().try_fold(HashMap::new(), |mut proofs, (addr, facts)| {
                if proofs.insert(addr, preimage_map_from_commitment_facts(facts)?).is_some() {
                    return Err(DeserializationError::KeyDuplicate(format!(
                        "duplicate storage trie witness address {addr:?}"
                    )));
                }
                Ok(proofs)
            })?;

        Ok(Self { classes_trie_proof, contracts_trie_proof, contracts_trie_storage_proofs })
    }
}

impl serde::Serialize for StarknetForestProofs {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let encoded = Self::serialize(self).map_err(serde::ser::Error::custom)?;
        encoded.0.serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for StarknetForestProofs {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes = Vec::<u8>::deserialize(deserializer)?;
        Self::deserialize(&DbValue(bytes)).map_err(DeError::custom)
    }
}

fn bincode_ser_err(error: bincode::Error) -> SerializationError {
    SerializationError::IOSerialize(std::io::Error::other(error))
}

fn preimage_map_from_commitment_facts(
    facts: RawPreimages,
) -> Result<PreimageMap, DeserializationError> {
    let mut preimage_map = PreimageMap::new();
    for (hash, raw_preimage) in facts {
        let preimage = Preimage::try_from(&raw_preimage).map_err(|error| {
            DeserializationError::ValueError(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                error.to_string(),
            )))
        })?;
        if preimage_map.insert(hash, preimage).is_some() {
            return Err(DeserializationError::KeyDuplicate(format!(
                "duplicate preimage node hash {hash:?}"
            )));
        }
    }
    Ok(preimage_map)
}
