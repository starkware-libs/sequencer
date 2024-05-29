use indexmap::IndexMap;
use once_cell::sync::Lazy;
use starknet_types_core::felt::Felt;

use crate::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce, StateDiffCommitment};
use crate::crypto::utils::HashChain;
use crate::hash::PoseidonHash;
use crate::state::{StorageKey, ThinStateDiff};
use crate::transaction_hash::ascii_as_felt;

#[cfg(test)]
#[path = "state_diff_hash_test.rs"]
mod state_diff_hash_test;

static STARKNET_STATE_DIFF0: Lazy<Felt> = Lazy::new(|| {
    ascii_as_felt("STARKNET_STATE_DIFF0").expect("ascii_as_felt failed for 'STARKNET_STATE_DIFF0'")
});

/// Poseidon(
///     "STARKNET_STATE_DIFF0", deployed_contracts, declared_classes, deprecated_declared_classes,
///     1, 0, storage_diffs, nonces
/// ).
pub fn calculate_state_diff_hash(state_diff: &ThinStateDiff) -> StateDiffCommitment {
    let mut hash_chain = HashChain::new();
    hash_chain = hash_chain.chain(&STARKNET_STATE_DIFF0);
    hash_chain = chain_updated_contracts(
        &state_diff.deployed_contracts,
        &state_diff.replaced_classes,
        hash_chain,
    );
    hash_chain = chain_declared_classes(&state_diff.declared_classes, hash_chain);
    hash_chain =
        chain_deprecated_declared_classes(&state_diff.deprecated_declared_classes, hash_chain);
    hash_chain = hash_chain.chain(&Felt::ONE) // placeholder.
        .chain(&Felt::ZERO); // placeholder.
    hash_chain = chain_storage_diffs(&state_diff.storage_diffs, hash_chain);
    hash_chain = chain_nonces(&state_diff.nonces, hash_chain);
    StateDiffCommitment(PoseidonHash(hash_chain.get_poseidon_hash()))
}

// Chains: [number_of_updated_contracts, address_0, class_hash_0, address_1, class_hash_1, ...].
// The updated contracts includes deployed contracts and replaced classes.
fn chain_updated_contracts(
    deployed_contracts: &IndexMap<ContractAddress, ClassHash>,
    replaced_classes: &IndexMap<ContractAddress, ClassHash>,
    mut hash_chain: HashChain,
) -> HashChain {
    let updated_contracts = deployed_contracts.iter().chain(replaced_classes.iter());
    hash_chain = hash_chain.chain(&(deployed_contracts.len() + replaced_classes.len()).into());
    for (address, class_hash) in sorted_index_map(&updated_contracts.collect()) {
        hash_chain = hash_chain.chain(&address.0).chain(class_hash);
    }
    hash_chain
}

// Chains: [number_of_declared_classes,
//      class_hash_0, compiled_class_hash_0, class_hash_1, compiled_class_hash_1, ...].
fn chain_declared_classes(
    declared_classes: &IndexMap<ClassHash, CompiledClassHash>,
    mut hash_chain: HashChain,
) -> HashChain {
    hash_chain = hash_chain.chain(&declared_classes.len().into());
    for (class_hash, compiled_class_hash) in sorted_index_map(declared_classes) {
        hash_chain = hash_chain.chain(&class_hash).chain(&compiled_class_hash.0)
    }
    hash_chain
}

// Chains: [number_of_old_declared_classes, class_hash_0, class_hash_1, ...].
fn chain_deprecated_declared_classes(
    deprecated_declared_classes: &[ClassHash],
    hash_chain: HashChain,
) -> HashChain {
    let mut sorted_deprecated_declared_classes = deprecated_declared_classes.to_vec();
    sorted_deprecated_declared_classes.sort_unstable();
    hash_chain
        .chain(&sorted_deprecated_declared_classes.len().into())
        .chain_iter(sorted_deprecated_declared_classes.iter().map(|class_hash| &class_hash.0))
}

// Chains: [number_of_updated_contracts,
//      contract_address_0, number_of_updates_in_contract_0, key_0, value0, key1, value1, ...,
//      contract_address_1, number_of_updates_in_contract_1, key_0, value0, key1, value1, ...,
// ]
fn chain_storage_diffs(
    storage_diffs: &IndexMap<ContractAddress, IndexMap<StorageKey, Felt>>,
    hash_chain: HashChain,
) -> HashChain {
    let mut n_updated_contracts = 0_u64;
    let mut storage_diffs_chain = HashChain::new();
    for (contract_address, key_value_map) in sorted_index_map(storage_diffs) {
        if key_value_map.is_empty() {
            // Filter out a contract with empty storage maps.
            continue;
        }
        n_updated_contracts += 1;
        storage_diffs_chain = storage_diffs_chain.chain(&contract_address);
        storage_diffs_chain = storage_diffs_chain.chain(&key_value_map.len().into());
        for (key, value) in sorted_index_map(&key_value_map) {
            storage_diffs_chain = storage_diffs_chain.chain(&key).chain(&value);
        }
    }
    hash_chain.chain(&n_updated_contracts.into()).extend(storage_diffs_chain)
}

// Chains: [number_of_updated_contracts nonces,
//      contract_address_0, nonce_0, contract_address_1, nonce_1, ...,
// ]
fn chain_nonces(nonces: &IndexMap<ContractAddress, Nonce>, mut hash_chain: HashChain) -> HashChain {
    hash_chain = hash_chain.chain(&nonces.len().into());
    for (contract_address, nonce) in sorted_index_map(nonces) {
        hash_chain = hash_chain.chain(&contract_address);
        hash_chain = hash_chain.chain(&nonce);
    }
    hash_chain
}

// Returns a clone of the map, sorted by keys.
fn sorted_index_map<K: Clone + std::cmp::Ord, V: Clone>(map: &IndexMap<K, V>) -> IndexMap<K, V> {
    let mut sorted_map = map.clone();
    sorted_map.sort_unstable_keys();
    sorted_map
}
