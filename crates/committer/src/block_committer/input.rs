use crate::felt::Felt;
use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::filled_tree::node::{ClassHash, CompiledClassHash, Nonce};
use crate::patricia_merkle_tree::node_data::leaf::{
    ContractState, LeafModifications, SkeletonLeaf,
};
use crate::patricia_merkle_tree::types::NodeIndex;
use crate::storage::storage_trait::{StorageKey, StorageValue};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
// TODO(Nimrod, 1/6/2024): Swap to starknet-types-core types once implemented.
pub struct ContractAddress(pub Felt);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
// TODO(Nimrod, 1/6/2024): Swap to starknet-types-core types once implemented.
pub struct StarknetStorageKey(pub Felt);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StarknetStorageValue(pub Felt);

#[derive(Debug, Default, Eq, PartialEq)]
pub struct StateDiff {
    pub address_to_class_hash: HashMap<ContractAddress, ClassHash>,
    pub address_to_nonce: HashMap<ContractAddress, Nonce>,
    pub class_hash_to_compiled_class_hash: HashMap<ClassHash, CompiledClassHash>,
    pub storage_updates:
        HashMap<ContractAddress, HashMap<StarknetStorageKey, StarknetStorageValue>>,
}

#[derive(Debug, Eq, PartialEq)]
pub struct Input {
    pub storage: HashMap<StorageKey, StorageValue>,
    /// All relevant information for the state diff commitment.
    pub state_diff: StateDiff,
    pub current_contracts_trie_leaves: HashMap<ContractAddress, ContractState>,
    pub contracts_trie_root_hash: HashOutput,
    pub classes_trie_root_hash: HashOutput,
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
                        .map(|(key, value)| {
                            (
                                NodeIndex::from_starknet_storage_key(key),
                                SkeletonLeaf::from(value.0),
                            )
                        })
                        .collect(),
                    None => HashMap::new(),
                };
                (**address, updates)
            })
            .collect()
    }

    pub(crate) fn skeleton_classes_updates(
        class_hash_to_compiled_class_hash: &HashMap<ClassHash, CompiledClassHash>,
    ) -> LeafModifications<SkeletonLeaf> {
        class_hash_to_compiled_class_hash
            .iter()
            .map(|(class_hash, compiled_class_hash)| {
                (
                    NodeIndex::from_class_hash(class_hash),
                    SkeletonLeaf::from(compiled_class_hash.0),
                )
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
                    Some(inner_updates) => inner_updates
                        .iter()
                        .map(|(key, value)| {
                            (
                                NodeIndex::from_starknet_storage_key(key),
                                StarknetStorageValue(value.0),
                            )
                        })
                        .collect(),
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
                (NodeIndex::from_class_hash(class_hash), *compiled_class_hash)
            })
            .collect()
    }
}
