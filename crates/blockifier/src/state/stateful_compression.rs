use std::collections::{BTreeSet, HashMap};
use std::sync::LazyLock;

use starknet_api::core::{ContractAddress, PatriciaKey};
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use super::cached_state::{CachedState, StateMaps, StorageEntry};
use super::state_api::{StateReader, StateResult};

#[cfg(test)]
#[path = "stateful_compression_test.rs"]
pub mod stateful_compression_test;

type Alias = Felt;
type AliasKey = StorageKey;

// The initial alias available for allocation.
const INITIAL_AVAILABLE_ALIAS: Felt = Felt::from_hex_unchecked("0x80");

// The address of the alias contract.
const ALIAS_CONTRACT_ADDRESS: ContractAddress = ContractAddress(PatriciaKey::TWO);
// The storage key of the alias counter in the alias contract.
const ALIAS_COUNTER_STORAGE_KEY: StorageKey = StorageKey(PatriciaKey::ZERO);
// The maximal contract address for which aliases are not used and all keys are serialized as is,
// without compression.
static MAX_NON_COMPRESSED_CONTRACT_ADDRESS: LazyLock<ContractAddress> = LazyLock::new(|| {
    ContractAddress(PatriciaKey::try_from(Felt::from_hex_unchecked("0xf")).unwrap())
});
// The minimal value for a key to be allocated an alias. Smaller keys are serialized as is (their
// alias is identical to the key).
static MIN_VALUE_FOR_ALIAS_ALLOC: LazyLock<PatriciaKey> =
    LazyLock::new(|| PatriciaKey::try_from(INITIAL_AVAILABLE_ALIAS).unwrap());

/// Allocates aliases for the new addresses and storage keys in the alias contract.
/// Iterates over the addresses in ascending order. For each address, sets an alias for the new
/// storage keys (in ascending order) and for the address itself.
pub fn state_diff_with_alias_allocation<S: StateReader>(
    state: &mut CachedState<S>,
) -> StateResult<StateMaps> {
    let mut state_diff = state.to_state_diff()?.state_maps;

    // Collect the addresses and the storage keys that need aliases.
    let addresses: BTreeSet<ContractAddress> =
        state_diff.get_modified_contracts().into_iter().collect();
    let mut sorted_storage_keys = HashMap::new();
    for (address, storage_key) in state_diff.storage.keys() {
        if address > &*MAX_NON_COMPRESSED_CONTRACT_ADDRESS {
            sorted_storage_keys.entry(address).or_insert_with(BTreeSet::new).insert(storage_key);
        }
    }

    // Iterate over the addresses and the storage keys and update the aliases.
    let mut alias_updater = AliasUpdater::new(state)?;
    for address in addresses {
        if let Some(storage_keys) = sorted_storage_keys.get(&address) {
            for key in storage_keys {
                alias_updater.insert_alias(key)?;
            }
        }
        alias_updater.insert_alias(&StorageKey(address.0))?;
    }

    let alias_storage_updates = alias_updater.finalize_updates();
    state_diff.storage.extend(alias_storage_updates);
    Ok(state_diff)
}

/// Generate updates for the alias contract with the new keys.
struct AliasUpdater<'a, S: StateReader> {
    state: &'a CachedState<S>,
    new_aliases: HashMap<AliasKey, Alias>,
    next_free_alias: Option<Alias>,
}

impl<'a, S: StateReader> AliasUpdater<'a, S> {
    fn new(state: &'a CachedState<S>) -> StateResult<Self> {
        let stored_counter =
            state.get_storage_at(ALIAS_CONTRACT_ADDRESS, ALIAS_COUNTER_STORAGE_KEY)?;
        Ok(Self {
            state,
            new_aliases: HashMap::new(),
            next_free_alias: if stored_counter == Felt::ZERO { None } else { Some(stored_counter) },
        })
    }

    /// Inserts the alias key to the updates if it's not already aliased.
    fn insert_alias(&mut self, alias_key: &AliasKey) -> StateResult<()> {
        if alias_key.0 >= *MIN_VALUE_FOR_ALIAS_ALLOC
            && self.state.get_storage_at(ALIAS_CONTRACT_ADDRESS, *alias_key)? == Felt::ZERO
            && !self.new_aliases.contains_key(alias_key)
        {
            let alias_to_allocate = match self.next_free_alias {
                Some(alias) => alias,
                None => INITIAL_AVAILABLE_ALIAS,
            };
            self.new_aliases.insert(*alias_key, alias_to_allocate);
            self.next_free_alias = Some(alias_to_allocate + Felt::ONE);
        }
        Ok(())
    }

    /// Inserts the counter of the alias contract. Returns the storage updates for the alias
    /// contract.
    fn finalize_updates(mut self) -> HashMap<StorageEntry, Felt> {
        match self.next_free_alias {
            None => {
                self.new_aliases.insert(ALIAS_COUNTER_STORAGE_KEY, INITIAL_AVAILABLE_ALIAS);
            }
            Some(alias) => {
                if !self.new_aliases.is_empty() {
                    self.new_aliases.insert(ALIAS_COUNTER_STORAGE_KEY, alias);
                }
            }
        }

        self.new_aliases
            .into_iter()
            .map(|(key, alias)| ((ALIAS_CONTRACT_ADDRESS, key), alias))
            .collect()
    }
}
