use starknet_api::core::{ContractAddress, PatriciaKey};
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use super::cached_state::CachedState;
use super::state_api::{State, StateReader, StateResult};

#[cfg(test)]
#[path = "stateful_compression_test.rs"]
pub mod stateful_compression_test;

type Alias = Felt;
type AliasKey = StorageKey;

// The address of the alias contract.
const ALIAS_CONTRACT_ADDRESS: u8 = 2;
// The storage key of the alias counter in the alias contract.
const ALIAS_COUNTER_STORAGE_KEY: u8 = 0;
// The minimal value for a key to be allocated an alias. Smaller keys are serialized as is (their
// alias is identical to the key).
const MIN_VALUE_FOR_ALIAS_ALLOC: Felt = Felt::from_hex_unchecked("0x80");

pub fn get_alias_contract_address() -> ContractAddress {
    ContractAddress::from(ALIAS_CONTRACT_ADDRESS)
}
pub fn get_alias_counter_storage_key() -> StorageKey {
    StorageKey::from(ALIAS_COUNTER_STORAGE_KEY)
}

/// Updates the alias contract with the new keys.
struct AliasUpdater<'a, S: StateReader> {
    state: &'a mut CachedState<S>,
    next_free_alias: Alias,
}

impl<'a, S: StateReader> AliasUpdater<'a, S> {
    fn new(state: &'a mut CachedState<S>) -> StateResult<Self> {
        let next_free_alias =
            state.get_storage_at(get_alias_contract_address(), get_alias_counter_storage_key())?;
        Ok(Self {
            state,
            next_free_alias: if next_free_alias == Felt::ZERO {
                // Aliasing first time.
                MIN_VALUE_FOR_ALIAS_ALLOC
            } else {
                next_free_alias
            },
        })
    }

    /// Inserts the alias key to the updates if it's not already aliased.
    fn set_alias(&mut self, alias_key: &AliasKey) -> StateResult<()> {
        if alias_key.0 >= PatriciaKey::try_from(MIN_VALUE_FOR_ALIAS_ALLOC)?
            && self.state.get_storage_at(get_alias_contract_address(), *alias_key)? == Felt::ZERO
        {
            self.state.set_storage_at(
                get_alias_contract_address(),
                *alias_key,
                self.next_free_alias,
            )?;
            self.next_free_alias += Felt::ONE;
        }
        Ok(())
    }

    /// Writes the counter of the alias contract in the storage.
    fn finalize_updates(self) -> StateResult<()> {
        self.state.set_storage_at(
            get_alias_contract_address(),
            get_alias_counter_storage_key(),
            self.next_free_alias,
        )
    }
}
