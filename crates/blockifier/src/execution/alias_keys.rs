#[cfg(test)]
#[path = "alias_keys_test.rs"]
pub mod test;

use std::collections::HashSet;

use cairo_vm::Felt252;
use starknet_api::core::{ContractAddress, PatriciaKey};
use starknet_api::hash::StarkHash;
use starknet_api::state::StorageKey;

use crate::state::cached_state::{CachedState, StateChanges};
use crate::state::state_api::{StateReader, StateResult};

const ALIAS_CONTRACT_ADDRESS: ContractAddress = ContractAddress(PatriciaKey(StarkHash::TWO));
const KEY_OF_NEXT_FREE_ALIAS: StorageKey = StorageKey(PatriciaKey(StarkHash::ZERO));

pub struct Alias(pub Felt252);

#[derive(Debug, Default, Copy, Clone, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct AliasKey(pub PatriciaKey);

impl From<AliasKey> for StorageKey {
    fn from(alias_key: AliasKey) -> StorageKey {
        StorageKey(alias_key.0)
    }
}

impl From<StorageKey> for AliasKey {
    fn from(storage_key: StorageKey) -> AliasKey {
        AliasKey(storage_key.0)
    }
}

impl From<ContractAddress> for AliasKey {
    fn from(contract_address: ContractAddress) -> AliasKey {
        AliasKey(contract_address.0)
    }
}

impl From<u128> for AliasKey {
    fn from(val: u128) -> Self {
        AliasKey(PatriciaKey::from(val))
    }
}

/// Returns the set of modified contracts and storage keys from the input state changes.
pub fn get_modified_contracts_and_storage_keys(state_changes: &StateChanges) -> HashSet<AliasKey> {
    // TODO: Filter out keys from specific contracts.
    let mut modified_keys: HashSet<AliasKey> =
        state_changes.0.storage.keys().map(|storage_key| storage_key.1.into()).collect();
    modified_keys.extend(state_changes.get_modified_contracts().into_iter().map(AliasKey::from));
    modified_keys
}

/// Returns the set of aliases from the input state changes, which are not yet in the alias
/// contract.
pub fn get_new_alias_keys<S: StateReader>(
    state: &CachedState<S>,
    state_changes: &StateChanges,
) -> StateResult<HashSet<AliasKey>> {
    // TODO: Remove low aliases.
    let modified_keys = get_modified_contracts_and_storage_keys(state_changes);
    let mut new_aliases = HashSet::new();
    for alias_key in modified_keys {
        if state.get_storage_at(ALIAS_CONTRACT_ADDRESS, alias_key.into())? == Felt252::ZERO {
            new_aliases.insert(alias_key);
        }
    }
    Ok(new_aliases)
}

pub fn get_alias_at<S: StateReader>(
    state: &CachedState<S>,
    alias_key: AliasKey,
) -> StateResult<Felt252> {
    state.get_storage_at(ALIAS_CONTRACT_ADDRESS, alias_key.into())
}

/// Returns the state changes of inserting the new aliases to the alias contract.
pub fn alias_state_changes<S: StateReader>(
    state: &CachedState<S>,
    new_aliases: &HashSet<AliasKey>,
) -> StateResult<StateChanges> {
    let mut state_changes = StateChanges::default();
    let mut next_alias = state.get_storage_at(ALIAS_CONTRACT_ADDRESS, KEY_OF_NEXT_FREE_ALIAS)?;
    // TODO: The first time the alias contract is used, set KEY_OF_NEXT_FREE_ALIAS.
    // TODO: Determine the order of the elements.
    for alias_key in new_aliases {
        state_changes
            .0
            .storage
            .insert((ALIAS_CONTRACT_ADDRESS, StorageKey::from(*alias_key)), next_alias);
        next_alias += Felt252::ONE;
    }
    if !new_aliases.is_empty() {
        state_changes
            .0
            .storage
            .insert((ALIAS_CONTRACT_ADDRESS, KEY_OF_NEXT_FREE_ALIAS), next_alias);
    }

    Ok(state_changes)
}

/// Returns the number of alias contract updates required to add.
pub fn n_alias_contract_updates(new_aliases: &HashSet<AliasKey>) -> usize {
    let n_new_aliases = new_aliases.len();
    match n_new_aliases {
        0 => 0,
        _ => n_new_aliases + 1, // +1 for the counter.
    }
}
