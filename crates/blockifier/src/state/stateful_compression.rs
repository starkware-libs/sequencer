use std::collections::HashMap;
use std::sync::LazyLock;

use starknet_api::core::{ContractAddress, PatriciaKey};
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use super::cached_state::{CachedState, StorageEntry};
use super::state_api::{StateReader, StateResult};

#[cfg(test)]
#[path = "stateful_compression_test.rs"]
pub mod stateful_compression_test;

type Alias = Felt;
type AliasKey = StorageKey;

// The initial alias available for allocation.
const INITIAL_AVAILABLE_ALIAS: Felt = Felt::from_hex_unchecked("0x80");

// The address of the alias contract.
static ALIAS_CONTRACT_ADDRESS: LazyLock<ContractAddress> =
    LazyLock::new(|| ContractAddress(PatriciaKey::try_from(Felt::TWO).unwrap()));
// The storage key of the alias counter in the alias contract.
static ALIAS_COUNTER_STORAGE_KEY: LazyLock<StorageKey> =
    LazyLock::new(|| StorageKey(PatriciaKey::try_from(Felt::ZERO).unwrap()));
// The minimal value for a key to be allocated an alias. Smaller keys are serialized as is (their
// alias is identical to the key).
static MIN_VALUE_FOR_ALIAS_ALLOC: LazyLock<PatriciaKey> =
    LazyLock::new(|| PatriciaKey::try_from(INITIAL_AVAILABLE_ALIAS).unwrap());

/// Generate updates for the alias contract with the new keys.
struct AliasUpdater<'a, S: StateReader> {
    state: &'a CachedState<S>,
    new_aliases: HashMap<AliasKey, Alias>,
    next_free_alias: Alias,
}

impl<'a, S: StateReader> AliasUpdater<'a, S> {
    fn new(state: &'a CachedState<S>) -> StateResult<Self> {
        let next_free_alias =
            state.get_storage_at(*ALIAS_CONTRACT_ADDRESS, *ALIAS_COUNTER_STORAGE_KEY)?;
        Ok(Self {
            state,
            new_aliases: HashMap::new(),
            next_free_alias: if next_free_alias == Felt::ZERO {
                // Aliasing first time.
                INITIAL_AVAILABLE_ALIAS
            } else {
                next_free_alias
            },
        })
    }

    /// Inserts the alias key to the updates if it's not already aliased.
    fn insert_alias(&mut self, alias_key: &AliasKey) -> StateResult<()> {
        if alias_key.0 >= *MIN_VALUE_FOR_ALIAS_ALLOC
            && self.state.get_storage_at(*ALIAS_CONTRACT_ADDRESS, *alias_key)? == Felt::ZERO
            && !self.new_aliases.contains_key(alias_key)
        {
            self.new_aliases.insert(*alias_key, self.next_free_alias);
            self.next_free_alias += Felt::ONE;
        }
        Ok(())
    }

    /// Inserts the counter of the alias contract. Returns the storage updates for the alias
    /// contract.
    fn finalize_updates(mut self) -> HashMap<StorageEntry, Felt> {
        if !self.new_aliases.is_empty() || self.next_free_alias == INITIAL_AVAILABLE_ALIAS {
            self.new_aliases.insert(*ALIAS_COUNTER_STORAGE_KEY, self.next_free_alias);
        }
        self.new_aliases
            .into_iter()
            .map(|(key, alias)| ((*ALIAS_CONTRACT_ADDRESS, key), alias))
            .collect()
    }
}
