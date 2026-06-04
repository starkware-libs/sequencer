use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use starknet_api::core::{ClassHash, ContractAddress};
use starknet_api::hash::{HashOutput, StateRoots};
use starknet_patricia::impl_from_hex_for_felt_wrapper;
use starknet_patricia::patricia_merkle_tree::filled_tree::tree::FilledTreeImpl;
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::{
    flatten_preimages,
    PreimageMap,
};
use starknet_patricia::patricia_merkle_tree::node_data::leaf::SkeletonLeaf;
use starknet_patricia::patricia_merkle_tree::types::{NodeIndex, SubTreeHeight};
#[cfg(feature = "os_input")]
use starknet_patricia_storage::errors::SerializationError;
use starknet_types_core::felt::{Felt, FromStrError};

use crate::block_committer::input::{try_node_index_into_contract_address, StarknetStorageValue};
use crate::db::db_layout::DbLayout;
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;

pub fn fixed_hex_string_no_prefix(felt: &Felt) -> String {
    format!("{felt:064x}")
}

pub fn class_hash_into_node_index(class_hash: &ClassHash) -> NodeIndex {
    NodeIndex::from_leaf_felt(&class_hash.0)
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CompiledClassHash(pub Felt);

impl From<CompiledClassHash> for SkeletonLeaf {
    fn from(compiled_class_hash: CompiledClassHash) -> Self {
        SkeletonLeaf::from(compiled_class_hash.0)
    }
}

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

#[derive(Clone, Debug, PartialEq)]
pub struct ContractsTrieProof {
    pub nodes: PreimageMap,
    pub leaves: HashMap<ContractAddress, ContractState>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StarknetForestProofs {
    pub classes_trie_proof: PreimageMap,
    pub contracts_trie_proof: ContractsTrieProof,
    pub contracts_trie_storage_proofs: HashMap<ContractAddress, PreimageMap>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommitmentInfo {
    pub previous_root: HashOutput,
    pub updated_root: HashOutput,
    pub tree_height: SubTreeHeight,
    // TODO(Dori, 1/8/2025): The value type here should probably be more specific (NodeData<L> for
    //   L: Leaf). This poses a problem in deserialization, as a serialized edge node and a
    //   serialized contract state leaf are both currently vectors of 3 field elements; as the
    //   semantics of the values are unimportant for the OS commitments, we make do with a vector
    //   of field elements as values for now.
    pub commitment_facts: HashMap<HashOutput, Vec<Felt>>,
}

#[cfg(any(feature = "testing", test))]
impl Default for CommitmentInfo {
    fn default() -> CommitmentInfo {
        CommitmentInfo {
            previous_root: HashOutput::default(),
            updated_root: HashOutput::default(),
            tree_height: SubTreeHeight::ACTUAL_HEIGHT,
            commitment_facts: HashMap::default(),
        }
    }
}

/// Contains all commitment information for a block's state trees.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[cfg_attr(any(feature = "testing", test), derive(Default))]
pub struct StateCommitmentInfos {
    pub contracts_trie_commitment_info: CommitmentInfo,
    pub classes_trie_commitment_info: CommitmentInfo,
    pub storage_tries_commitment_infos: HashMap<ContractAddress, CommitmentInfo>,
}

#[cfg(feature = "os_input")]
#[derive(Debug, thiserror::Error)]
pub enum StateCommitmentInfosCodecError {
    #[error(transparent)]
    Bincode(#[from] bincode::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[cfg(feature = "os_input")]
impl From<StateCommitmentInfosCodecError> for SerializationError {
    fn from(error: StateCommitmentInfosCodecError) -> Self {
        match error {
            StateCommitmentInfosCodecError::Bincode(error) => {
                SerializationError::IOSerialize(std::io::Error::other(error))
            }
            StateCommitmentInfosCodecError::Io(error) => SerializationError::IOSerialize(error),
        }
    }
}

impl StateCommitmentInfos {
    /// Bincode-serializes and zstd-compresses the commitment infos into a byte vector.
    #[cfg(feature = "os_input")]
    pub fn compress(&self) -> Result<Vec<u8>, StateCommitmentInfosCodecError> {
        let bincode_payload = bincode::serialize(self)?;
        Ok(zstd::encode_all(bincode_payload.as_slice(), zstd::DEFAULT_COMPRESSION_LEVEL)?)
    }

    /// Reverses [`StateCommitmentInfos::compress`]: zstd-decompresses then bincode-deserializes.
    #[cfg(feature = "os_input")]
    pub fn decompress(data: &[u8]) -> Result<Self, StateCommitmentInfosCodecError> {
        let bincode_payload = zstd::decode_all(data)?;
        Ok(bincode::deserialize(&bincode_payload)?)
    }

    /// Builds the commitment infos directly from the pre- and post-commit state roots and the
    /// merged Patricia witness proofs gathered during a commit, without re-reading the tries.
    ///
    /// `previous_storage_roots` holds each accessed contract's storage root in the *pre-commit*
    /// contracts trie. The post-commit storage roots are read from
    /// `merged_proofs.contracts_trie_proof.leaves`. The `commitment_facts` are the flattened
    /// preimages from `merged_proofs`.
    pub fn from_commit_witnesses(
        previous_state_roots: &StateRoots,
        new_state_roots: &StateRoots,
        previous_storage_roots: &HashMap<ContractAddress, HashOutput>,
        merged_proofs: &StarknetForestProofs,
    ) -> Self {
        let contracts_trie_commitment_info = CommitmentInfo {
            previous_root: previous_state_roots.contracts_trie_root_hash,
            updated_root: new_state_roots.contracts_trie_root_hash,
            tree_height: SubTreeHeight::ACTUAL_HEIGHT,
            commitment_facts: flatten_preimages(&merged_proofs.contracts_trie_proof.nodes),
        };
        let classes_trie_commitment_info = CommitmentInfo {
            previous_root: previous_state_roots.classes_trie_root_hash,
            updated_root: new_state_roots.classes_trie_root_hash,
            tree_height: SubTreeHeight::ACTUAL_HEIGHT,
            commitment_facts: flatten_preimages(&merged_proofs.classes_trie_proof),
        };
        // Iterate the post-commit contract leaves, which cover every accessed contract (including
        // ones newly deployed in this block). A contract absent from `previous_storage_roots` did
        // not exist pre-commit, so its previous storage root is the empty-tree root.
        let storage_tries_commitment_infos = merged_proofs
            .contracts_trie_proof
            .leaves
            .iter()
            .map(|(address, new_contract_state)| {
                let previous_root = previous_storage_roots
                    .get(address)
                    .copied()
                    .unwrap_or(HashOutput::ROOT_OF_EMPTY_TREE);
                // Not all accessed contracts have storage proofs (e.g. a contract whose nonce
                // changed but no storage slot did).
                let commitment_facts = merged_proofs
                    .contracts_trie_storage_proofs
                    .get(address)
                    .map_or_else(HashMap::new, flatten_preimages);
                (
                    *address,
                    CommitmentInfo {
                        previous_root,
                        updated_root: new_contract_state.storage_root_hash,
                        tree_height: SubTreeHeight::ACTUAL_HEIGHT,
                        commitment_facts,
                    },
                )
            })
            .collect();

        Self {
            contracts_trie_commitment_info,
            classes_trie_commitment_info,
            storage_tries_commitment_infos,
        }
    }

    /// Total number of `commitment_facts` entries across all tries (for logging/metrics).
    #[cfg(feature = "os_input")]
    pub fn n_commitment_facts(&self) -> usize {
        self.contracts_trie_commitment_info.commitment_facts.len()
            + self.classes_trie_commitment_info.commitment_facts.len()
            + self
                .storage_tries_commitment_infos
                .values()
                .map(|info| info.commitment_facts.len())
                .sum::<usize>()
    }
}

impl StarknetForestProofs {
    pub fn build<Layout>(
        classes_trie_proof: PreimageMap,
        contracts_proof_nodes: PreimageMap,
        contract_leaves: HashMap<NodeIndex, Layout::ContractStateDbLeaf>,
        contracts_trie_storage_proofs: HashMap<ContractAddress, PreimageMap>,
    ) -> Self
    where
        Layout: DbLayout,
        Layout::ContractStateDbLeaf: Into<ContractState>,
    {
        // Convert contract_leaves_data keys from NodeIndex to ContractAddress.
        let contract_leaves_data: HashMap<ContractAddress, ContractState> = contract_leaves
            .into_iter()
            .map(|(idx, contract_state_leaf)| {
                (
                    try_node_index_into_contract_address(&idx).unwrap_or_else(|_| {
                        panic!(
                            "Converting leaf NodeIndex to ContractAddress should succeed; failed \
                             to convert {idx:?}."
                        )
                    }),
                    contract_state_leaf.into(),
                )
            })
            .collect();

        Self {
            classes_trie_proof,
            contracts_trie_proof: ContractsTrieProof {
                nodes: contracts_proof_nodes,
                leaves: contract_leaves_data,
            },
            contracts_trie_storage_proofs,
        }
    }

    pub fn extend(&mut self, other: Self) {
        self.classes_trie_proof.extend(other.classes_trie_proof);
        self.contracts_trie_proof.nodes.extend(other.contracts_trie_proof.nodes);
        self.contracts_trie_proof.leaves.extend(other.contracts_trie_proof.leaves);
        for (address, proof) in other.contracts_trie_storage_proofs {
            self.contracts_trie_storage_proofs.entry(address).or_default().extend(proof);
        }
    }

    pub fn get_nodes_count(&self) -> usize {
        self.classes_trie_proof.len()
            + self.contracts_trie_proof.nodes.len()
            + self.contracts_trie_proof.leaves.len()
            + self
                .contracts_trie_storage_proofs
                .values()
                .fold(0, |count, proofs| count + proofs.len())
    }
}

pub struct RootHashes {
    pub previous_root_hash: HashOutput,
    pub new_root_hash: HashOutput,
}
