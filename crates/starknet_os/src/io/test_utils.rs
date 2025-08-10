use blockifier::state::cached_state::StateMaps;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use crate::io::os_output_types::{
    FullCompiledClassHashUpdate,
    FullContractChanges,
    FullContractStorageUpdate,
    FullOsStateDiff,
    PartialCompiledClassHashUpdate,
    PartialContractChanges,
    PartialContractStorageUpdate,
    PartialOsStateDiff,
};

// Getters
pub(crate) trait UpdateGetter<K, V> {
    fn key(&self) -> K;
    fn new_value(&self) -> V;
}

impl UpdateGetter<StorageKey, Felt> for FullContractStorageUpdate {
    fn key(&self) -> StorageKey {
        self.key
    }

    fn new_value(&self) -> Felt {
        self.new_value
    }
}

impl UpdateGetter<StorageKey, Felt> for PartialContractStorageUpdate {
    fn key(&self) -> StorageKey {
        self.key
    }

    fn new_value(&self) -> Felt {
        self.new_value
    }
}

impl UpdateGetter<ClassHash, CompiledClassHash> for FullCompiledClassHashUpdate {
    fn key(&self) -> ClassHash {
        self.class_hash
    }

    fn new_value(&self) -> CompiledClassHash {
        self.next_compiled_class_hash
    }
}

impl UpdateGetter<ClassHash, CompiledClassHash> for PartialCompiledClassHashUpdate {
    fn key(&self) -> ClassHash {
        self.class_hash
    }

    fn new_value(&self) -> CompiledClassHash {
        self.next_compiled_class_hash
    }
}

pub(crate) trait ContractChangesGetter {
    fn addr(&self) -> ContractAddress;
    fn new_nonce(&self) -> Option<Nonce>;
    fn new_class_hash(&self) -> Option<ClassHash>;
    fn storage_changes(&self) -> &[impl UpdateGetter<StorageKey, Felt>];
}

impl ContractChangesGetter for FullContractChanges {
    fn addr(&self) -> ContractAddress {
        self.addr
    }

    fn new_nonce(&self) -> Option<Nonce> {
        Some(self.new_nonce)
    }

    fn new_class_hash(&self) -> Option<ClassHash> {
        Some(self.new_class_hash)
    }

    fn storage_changes(&self) -> &[impl UpdateGetter<StorageKey, Felt>] {
        &self.storage_changes
    }
}

impl ContractChangesGetter for PartialContractChanges {
    fn addr(&self) -> ContractAddress {
        self.addr
    }

    fn new_nonce(&self) -> Option<Nonce> {
        self.new_nonce
    }

    fn new_class_hash(&self) -> Option<ClassHash> {
        self.new_class_hash
    }

    fn storage_changes(&self) -> &[impl UpdateGetter<StorageKey, Felt>] {
        &self.storage_changes
    }
}

fn to_state_maps<CO: ContractChangesGetter, CL: UpdateGetter<ClassHash, CompiledClassHash>>(
    contracts: &[CO],
    classes: &[CL],
) -> StateMaps {
    let class_hashes = contracts
        .iter()
        .filter_map(|contract| {
            contract.new_class_hash().map(|class_hash| (contract.addr(), class_hash))
        })
        .collect();
    let nonces = contracts
        .iter()
        .filter_map(|contract| contract.new_nonce().map(|nonce| (contract.addr(), nonce)))
        .collect();
    let mut storage = std::collections::HashMap::new();
    for contract in contracts {
        for change in contract.storage_changes() {
            storage.insert((contract.addr(), change.key()), change.new_value());
        }
    }
    let compiled_class_hashes = classes
        .iter()
        .map(|class_hash_update| (class_hash_update.key(), class_hash_update.new_value()))
        .collect();
    let declared_contracts = std::collections::HashMap::new();
    StateMaps { nonces, class_hashes, storage, compiled_class_hashes, declared_contracts }
}

impl FullOsStateDiff {
    pub fn as_state_maps(&self) -> StateMaps {
        to_state_maps(&self.contracts, &self.classes)
    }
}

impl PartialOsStateDiff {
    pub fn as_state_maps(&self) -> StateMaps {
        to_state_maps(&self.contracts, &self.classes)
    }
}
