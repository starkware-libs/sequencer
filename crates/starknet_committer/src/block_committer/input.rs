use std::collections::{HashMap, HashSet};
use std::fmt::Debug;

use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_api::state::StorageKey;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::{LeafModifications, SkeletonLeaf};
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;
use starknet_patricia_storage::storage_trait::{DbKey, DbValue};
use starknet_types_core::felt::Felt;
use tracing::level_filters::LevelFilter;

use crate::patricia_merkle_tree::types::{class_hash_into_node_index, CompiledClassHash};

#[cfg(test)]
#[path = "input_test.rs"]
pub mod input_test;

pub fn try_node_index_into_contract_address(
    node_index: &NodeIndex,
) -> Result<ContractAddress, String> {
    if !node_index.is_leaf() {
        return Err("NodeIndex is not a leaf.".to_string());
    }
    let result = Felt::try_from(*node_index - NodeIndex::FIRST_LEAF);
    match result {
        Ok(felt) => Ok(ContractAddress::try_from(felt).map_err(|error| error.to_string())?),
        Err(error) => Err(format!(
            "Tried to convert node index to felt and got the following error: {:?}",
            error.to_string()
        )),
    }
}

pub fn contract_address_into_node_index(address: &ContractAddress) -> NodeIndex {
    NodeIndex::from_leaf_felt(&address.0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
// TODO(Nimrod, 1/6/2025):  Use the StarknetStorageValue defined in starknet-types-core when
// available.
pub struct StarknetStorageKey(pub StorageKey);

impl From<&StarknetStorageKey> for NodeIndex {
    fn from(key: &StarknetStorageKey) -> NodeIndex {
        NodeIndex::from_leaf_felt(&key.0)
    }
}

#[derive(Clone, Copy, Default, Debug, Eq, PartialEq)]
pub struct StarknetStorageValue(pub Felt);

#[derive(Debug, Default, Eq, PartialEq)]
pub struct StateDiff {
    pub address_to_class_hash: HashMap<ContractAddress, ClassHash>,
    pub address_to_nonce: HashMap<ContractAddress, Nonce>,
    pub class_hash_to_compiled_class_hash: HashMap<ClassHash, CompiledClassHash>,
    pub storage_updates:
        HashMap<ContractAddress, HashMap<StarknetStorageKey, StarknetStorageValue>>,
}

/// Trait contains all optional configurations of the committer.
pub trait Config: Debug + Eq + PartialEq {
    /// Indicates whether a warning should be given in case of a trivial state update.
    /// If the configuration is set, it requires that the storage will contain the original data for
    /// the modified leaves. Otherwise, it is not required.
    fn warn_on_trivial_modifications(&self) -> bool;

    /// Indicates from which log level output should be printed out to console.
    fn logger_level(&self) -> LevelFilter;
}

#[derive(Debug, Eq, PartialEq)]
pub struct ConfigImpl {
    warn_on_trivial_modifications: bool,
    log_level: LevelFilter,
}

impl Config for ConfigImpl {
    fn warn_on_trivial_modifications(&self) -> bool {
        self.warn_on_trivial_modifications
    }

    fn logger_level(&self) -> LevelFilter {
        self.log_level
    }
}

impl ConfigImpl {
    pub fn new(warn_on_trivial_modifications: bool, log_level: LevelFilter) -> Self {
        Self { warn_on_trivial_modifications, log_level }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct Input<C: Config> {
    pub storage: HashMap<DbKey, DbValue>,
    /// All relevant information for the state diff commitment.
    pub state_diff: StateDiff,
    pub contracts_trie_root_hash: HashOutput,
    pub classes_trie_root_hash: HashOutput,
    pub config: C,
}

impl StateDiff {
    pub(crate) fn accessed_addresses(&self) -> HashSet<&ContractAddress> {
        HashSet::from_iter(
            self.address_to_class_hash
                .keys()
                .chain(self.address_to_nonce.keys())
                .chain(self.storage_updates.keys()),
        )
    }

    /// For each modified contract calculates it's actual storage updates.
    pub(crate) fn skeleton_storage_updates(
        &self,
    ) -> HashMap<ContractAddress, LeafModifications<SkeletonLeaf>> {
        self.accessed_addresses()
            .iter()
            .map(|address| {
                let updates = match self.storage_updates.get(address) {
                    Some(inner_updates) => inner_updates
                        .iter()
                        .map(|(key, value)| (key.into(), SkeletonLeaf::from(value.0)))
                        .collect(),
                    None => HashMap::new(),
                };
                (**address, updates)
            })
            .collect()
    }

    pub(crate) fn skeleton_classes_updates(&self) -> LeafModifications<SkeletonLeaf> {
        self.class_hash_to_compiled_class_hash
            .iter()
            .map(|(class_hash, compiled_class_hash)| {
                (class_hash_into_node_index(class_hash), SkeletonLeaf::from(compiled_class_hash.0))
            })
            .collect()
    }

    pub(crate) fn actual_storage_updates(
        &self,
    ) -> HashMap<ContractAddress, LeafModifications<StarknetStorageValue>> {
        self.accessed_addresses()
            .iter()
            .map(|address| {
                let updates = match self.storage_updates.get(address) {
                    Some(inner_updates) => {
                        inner_updates.iter().map(|(key, value)| (key.into(), *value)).collect()
                    }
                    None => HashMap::new(),
                };
                (**address, updates)
            })
            .collect()
    }

    pub(crate) fn actual_classes_updates(&self) -> LeafModifications<CompiledClassHash> {
        self.class_hash_to_compiled_class_hash
            .iter()
            .map(|(class_hash, compiled_class_hash)| {
                (class_hash_into_node_index(class_hash), *compiled_class_hash)
            })
            .collect()
    }
}
