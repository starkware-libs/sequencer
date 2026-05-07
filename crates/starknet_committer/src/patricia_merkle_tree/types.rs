use std::collections::HashMap;

#[cfg(feature = "os_input")]
use ethnum::U256;
use serde::{Deserialize, Serialize};
#[cfg(feature = "os_input")]
use starknet_api::core::Nonce;
use starknet_api::core::{ClassHash, ContractAddress};
use starknet_api::hash::HashOutput;
use starknet_patricia::impl_from_hex_for_felt_wrapper;
use starknet_patricia::patricia_merkle_tree::filled_tree::tree::FilledTreeImpl;
#[cfg(feature = "os_input")]
use starknet_patricia::patricia_merkle_tree::node_data::errors::{
    EdgePathError,
    PathToBottomError,
};
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::PreimageMap;
#[cfg(feature = "os_input")]
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::{
    BinaryData,
    EdgeData,
    EdgePath,
    EdgePathLength,
    PathToBottom,
    Preimage,
};
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;
#[cfg(feature = "os_input")]
use starknet_patricia_storage::errors::{
    DeserializationError,
    SerializationError,
    SerializationResult,
};
#[cfg(feature = "os_input")]
use starknet_patricia_storage::storage_trait::DbValue;
use starknet_types_core::felt::{Felt, FromStrError};

use crate::block_committer::input::StarknetStorageValue;
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;

pub fn fixed_hex_string_no_prefix(felt: &Felt) -> String {
    format!("{felt:064x}")
}

pub fn class_hash_into_node_index(class_hash: &ClassHash) -> NodeIndex {
    NodeIndex::from_leaf_felt(&class_hash.0)
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CompiledClassHash(pub Felt);

impl AsRef<CompiledClassHash> for CompiledClassHash {
    fn as_ref(&self) -> &CompiledClassHash {
        self
    }
}

impl_from_hex_for_felt_wrapper!(CompiledClassHash);

pub type StorageTrie = FilledTreeImpl<StarknetStorageValue>;
pub type ClassesTrie = FilledTreeImpl<CompiledClassHash>;
pub type ContractsTrie = FilledTreeImpl<ContractState>;
pub type StorageTrieMap = HashMap<ContractAddress, StorageTrie>;

#[derive(Debug, PartialEq)]
pub struct ContractsTrieProof {
    pub nodes: PreimageMap,
    pub leaves: HashMap<ContractAddress, ContractState>,
}

#[derive(Debug, PartialEq)]
pub struct StarknetForestProofs {
    pub classes_trie_proof: PreimageMap,
    pub contracts_trie_proof: ContractsTrieProof,
    pub contracts_trie_storage_proofs: HashMap<ContractAddress, PreimageMap>,
}

impl StarknetForestProofs {
    pub fn extend(&mut self, other: Self) {
        self.classes_trie_proof.extend(other.classes_trie_proof);
        self.contracts_trie_proof.nodes.extend(other.contracts_trie_proof.nodes);
        self.contracts_trie_proof.leaves.extend(other.contracts_trie_proof.leaves);
        for (address, proof) in other.contracts_trie_storage_proofs {
            self.contracts_trie_storage_proofs.entry(address).or_default().extend(proof);
        }
    }

    /// Bincode payload for the OS-input witness KV (structured proofs, round-trips with
    /// [`Self::deserialize`]).
    ///
    /// The serialization is bincode for a 4-tuple, in order:
    ///
    /// 1. Classes trie inner nodes — `Vec<(HashOutput, encoded_preimage)>`, sorted by node hash.
    /// 2. Contract trie inner nodes — `Vec<(HashOutput, encoded_preimage)>`, sorted by node hash.
    /// 3. Contract trie leaves — `Vec<(ContractAddress, (Nonce, HashOutput, ClassHash))>`, sorted
    ///    by contract address.
    /// 4. Storage tries inner nodes — `Vec<(ContractAddress, Vec<(HashOutput,
    ///    encoded_preimage)>)>`, sorted by contract address; inner node lists are sorted by hash.
    ///
    /// Each `encoded_preimage` is a tag byte followed by a nested bincode blob:
    ///
    /// - `0` (binary) — `(left: HashOutput, right: HashOutput)`.
    /// - `1` (edge) — `(bottom: HashOutput, path: [u8; 32], length: u8)` where `path` is
    ///   `path_to_bottom.path` big-endian and `length` is `path_to_bottom.length`.
    #[cfg(feature = "os_input")]
    pub fn serialize(&self) -> SerializationResult<DbValue> {
        let classes = sorted_encoded_preimage_map(&self.classes_trie_proof)?;
        let contract_nodes = sorted_encoded_preimage_map(&self.contracts_trie_proof.nodes)?;
        let mut contract_leaves: Vec<(ContractAddress, (Nonce, HashOutput, ClassHash))> = self
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
        contract_leaves.sort_by(|(a, _), (b, _)| a.cmp(b));

        let mut storage: Vec<(ContractAddress, Vec<(HashOutput, Vec<u8>)>)> = self
            .contracts_trie_storage_proofs
            .iter()
            .map(|(addr, preimage_map)| {
                sorted_encoded_preimage_map(preimage_map).map(|encoded| (*addr, encoded))
            })
            .collect::<Result<_, _>>()?;

        storage.sort_by(|(a, _), (b, _)| a.cmp(b));

        bincode::serialize(&(classes, contract_nodes, contract_leaves, storage))
            .map(DbValue)
            .map_err(bincode_ser_err)
    }

    #[cfg(feature = "os_input")]
    pub fn deserialize(value: &DbValue) -> Result<Self, DeserializationError> {
        let (classes, contract_nodes, contract_leaves, storage): (
            Vec<(HashOutput, Vec<u8>)>,
            Vec<(HashOutput, Vec<u8>)>,
            Vec<(ContractAddress, (Nonce, HashOutput, ClassHash))>,
            Vec<(ContractAddress, Vec<(HashOutput, Vec<u8>)>)>,
        ) = bincode::deserialize(&value.0).map_err(|e| {
            DeserializationError::ValueError(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                e.to_string(),
            )))
        })?;

        let classes_trie_proof = preimage_map_from_encoded(classes)?;
        let contracts_trie_proof = ContractsTrieProof {
            nodes: preimage_map_from_encoded(contract_nodes)?,
            leaves: contract_leaves.into_iter().try_fold(
                HashMap::new(),
                |mut m, (addr, (nonce, storage_root_hash, class_hash))| {
                    if m.insert(addr, ContractState { nonce, storage_root_hash, class_hash })
                        .is_some()
                    {
                        return Err(DeserializationError::KeyDuplicate(format!(
                            "duplicate contracts trie leaf {addr:?}"
                        )));
                    }
                    Ok(m)
                },
            )?,
        };

        let mut contracts_trie_storage_proofs = HashMap::new();
        for (addr, entries) in storage {
            if contracts_trie_storage_proofs
                .insert(addr, preimage_map_from_encoded(entries)?)
                .is_some()
            {
                return Err(DeserializationError::KeyDuplicate(format!(
                    "duplicate storage trie witness address {addr:?}"
                )));
            }
        }

        Ok(Self { classes_trie_proof, contracts_trie_proof, contracts_trie_storage_proofs })
    }
}

pub struct RootHashes {
    pub previous_root_hash: HashOutput,
    pub new_root_hash: HashOutput,
}

#[cfg(feature = "os_input")]
const WITNESS_PREIMAGE_BINARY: u8 = 0;
#[cfg(feature = "os_input")]
const WITNESS_PREIMAGE_EDGE: u8 = 1;

#[cfg(feature = "os_input")]
fn bincode_ser_err(e: bincode::Error) -> SerializationError {
    SerializationError::IOSerialize(std::io::Error::other(e))
}

#[cfg(feature = "os_input")]
fn encode_preimage(p: &Preimage) -> Result<Vec<u8>, SerializationError> {
    match p {
        Preimage::Binary(b) => {
            let payload =
                bincode::serialize(&(b.left_data, b.right_data)).map_err(bincode_ser_err)?;
            let mut out = Vec::with_capacity(1 + payload.len());
            out.push(WITNESS_PREIMAGE_BINARY);
            out.extend_from_slice(&payload);
            Ok(out)
        }
        Preimage::Edge(e) => {
            let path_bytes = e.path_to_bottom.path.0.to_be_bytes();
            let payload =
                bincode::serialize(&(e.bottom_data, path_bytes, u8::from(e.path_to_bottom.length)))
                    .map_err(bincode_ser_err)?;
            let mut out = Vec::with_capacity(1 + payload.len());
            out.push(WITNESS_PREIMAGE_EDGE);
            out.extend_from_slice(&payload);
            Ok(out)
        }
    }
}

#[cfg(feature = "os_input")]
fn decode_preimage(encoded: &[u8]) -> Result<Preimage, DeserializationError> {
    let Some((&tag, payload)) = encoded.split_first() else {
        return Err(DeserializationError::ValueError(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "empty encoded preimage",
        ))));
    };
    match tag {
        WITNESS_PREIMAGE_BINARY => {
            let (left, right): (HashOutput, HashOutput) =
                bincode::deserialize(payload).map_err(|e| {
                    DeserializationError::ValueError(Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        e.to_string(),
                    )))
                })?;
            Ok(Preimage::Binary(BinaryData { left_data: left, right_data: right }))
        }
        WITNESS_PREIMAGE_EDGE => {
            let (bottom, path_bytes, length_u8): (HashOutput, [u8; 32], u8) =
                bincode::deserialize(payload).map_err(|e| {
                    DeserializationError::ValueError(Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        e.to_string(),
                    )))
                })?;
            let path = EdgePath(U256::from_be_bytes(path_bytes));
            let length = EdgePathLength::new(length_u8).map_err(|e: EdgePathError| {
                DeserializationError::ValueError(Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    e.to_string(),
                )))
            })?;
            let path_to_bottom =
                PathToBottom::new(path, length).map_err(|e: PathToBottomError| {
                    DeserializationError::ValueError(Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        e.to_string(),
                    )))
                })?;
            Ok(Preimage::Edge(EdgeData { bottom_data: bottom, path_to_bottom }))
        }
        other => Err(DeserializationError::ValueError(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("unknown preimage tag {other}"),
        )))),
    }
}

#[cfg(feature = "os_input")]
fn sorted_encoded_preimage_map(
    preimage_map: &PreimageMap,
) -> Result<Vec<(HashOutput, Vec<u8>)>, SerializationError> {
    let mut encoded_map = Vec::with_capacity(preimage_map.len());
    for (hash, preimage) in preimage_map {
        let encoded = encode_preimage(preimage)?;
        encoded_map.push((*hash, encoded));
    }
    encoded_map.sort_by(|(a, _), (b, _)| a.0.cmp(&b.0));
    Ok(encoded_map)
}

#[cfg(feature = "os_input")]
fn preimage_map_from_encoded(
    encoded_map: Vec<(HashOutput, Vec<u8>)>,
) -> Result<PreimageMap, DeserializationError> {
    let mut preimage_map = PreimageMap::new();
    for (hash, buf) in encoded_map {
        if preimage_map.insert(hash, decode_preimage(&buf)?).is_some() {
            return Err(DeserializationError::KeyDuplicate(format!(
                "duplicate preimage node hash {hash:?}"
            )));
        }
    }
    Ok(preimage_map)
}
