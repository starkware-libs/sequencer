use starknet_api::core::ClassHash;
use starknet_types_core::felt::Felt;

use crate::state::cached_state::{CachedState, StateChanges};
use crate::state::state_api::{StateReader, StateResult};

#[cfg(test)]
#[path = "alias_keys_test.rs"]
mod test;

/// Keys in contract addresses up to this address don't get aliases.
pub const N_SAVED_CONTRACT_ADDRESSES: u8 = 16;
/// The alias of a felt X up to this number is X.
/// This trivial mappings don't write to the alias contract.
pub const N_TRIVIAL_SELF_ALIASES: u8 = 128;

/// Returns the number of aliases we charge the transaction for.
/// Counts declared classes, deployed contracts, and storage keys that were previously empty and
/// are now filled.
pub fn n_charged_invoke_aliases<S: StateReader>(
    state: &CachedState<S>,
    state_changes: &StateChanges,
) -> StateResult<usize> {
    let n_declared_classes = state_changes
        .0
        .declared_contracts
        .iter()
        .filter(|(class_hash, is_declared)| {
            **is_declared && (class_hash.0 >= N_TRIVIAL_SELF_ALIASES.into())
        })
        .count();

    let mut n_deployed_contracts = 0;
    for contract_address in state_changes.0.class_hashes.keys() {
        if contract_address.0 >= N_TRIVIAL_SELF_ALIASES.into()
            // The contract is deployed, not replaced class.
            && state.get_class_hash_at(*contract_address)? == ClassHash(Felt::ZERO)
        {
            n_deployed_contracts += 1;
        }
    }

    let storage_changes = &state_changes.0.storage;
    let mut n_storage_keys = 0;
    for ((contract_address, storage_key), new_value) in storage_changes {
        if contract_address.0.0 >= N_SAVED_CONTRACT_ADDRESSES.into()
            && storage_key.0.0 >= N_TRIVIAL_SELF_ALIASES.into()
            // Zero to non-zero.
            && state.get_storage_at(*contract_address, *storage_key)? == Felt::ZERO
            && new_value != &Felt::ZERO
        {
            n_storage_keys += 1;
        }
    }

    Ok(n_declared_classes + n_deployed_contracts + n_storage_keys)
}
