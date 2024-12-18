use std::collections::{BTreeSet, HashMap};

use starknet_api::core::{ContractAddress, PatriciaKey};
use starknet_api::state::StorageKey;
use starknet_api::StarknetApiError;
use starknet_types_core::felt::Felt;
use thiserror::Error;

use super::cached_state::{CachedState, StateMaps, StorageEntry};
use super::errors::StateError;
use super::state_api::{StateReader, StateResult};

#[cfg(test)]
#[path = "stateful_compression_test.rs"]
pub mod stateful_compression_test;

type Alias = Felt;
type AliasKey = StorageKey;

#[derive(Debug, Error)]
pub enum CompressionError {
    #[error("Missing key in alias contract: {:#064x}", ***.0)]
    MissedAlias(AliasKey),
    #[error(transparent)]
    StateError(#[from] StateError),
    #[error(transparent)]
    StarknetApiError(#[from] StarknetApiError),
}
pub type CompressionResult<T> = Result<T, CompressionError>;

// The initial alias available for allocation.
const INITIAL_AVAILABLE_ALIAS_HEX: &str = "0x80";
const INITIAL_AVAILABLE_ALIAS: Felt = Felt::from_hex_unchecked(INITIAL_AVAILABLE_ALIAS_HEX);

// The storage key of the alias counter in the alias contract.
const ALIAS_COUNTER_STORAGE_KEY: StorageKey = StorageKey(PatriciaKey::ZERO);
// The maximal contract address for which aliases are not used and all keys are serialized as is,
// without compression.
const MAX_NON_COMPRESSED_CONTRACT_ADDRESS: ContractAddress =
    ContractAddress(PatriciaKey::from_hex_unchecked("0xf"));
// The minimal value for a key to be allocated an alias. Smaller keys are serialized as is (their
// alias is identical to the key).
const MIN_VALUE_FOR_ALIAS_ALLOC: PatriciaKey =
    PatriciaKey::from_hex_unchecked(INITIAL_AVAILABLE_ALIAS_HEX);

/// Allocates aliases for the new addresses and storage keys in the alias contract.
/// Iterates over the addresses in ascending order. For each address, sets an alias for the new
/// storage keys (in ascending order) and for the address itself.
pub fn state_diff_with_alias_allocation<S: StateReader>(
    state: &mut CachedState<S>,
    alias_contract_address: ContractAddress,
) -> StateResult<StateMaps> {
    let mut state_diff = state.to_state_diff()?.state_maps;

    // Collect the contract addresses and the storage keys that need aliases.
    let contract_addresses: BTreeSet<ContractAddress> =
        state_diff.get_modified_contracts().into_iter().collect();
    let mut contract_address_to_sorted_storage_keys = HashMap::new();
    for (contract_address, storage_key) in state_diff.storage.keys() {
        if contract_address > &MAX_NON_COMPRESSED_CONTRACT_ADDRESS {
            contract_address_to_sorted_storage_keys
                .entry(contract_address)
                .or_insert_with(BTreeSet::new)
                .insert(storage_key);
        }
    }

    // Iterate over the addresses and the storage keys and update the aliases.
    let mut alias_updater = AliasUpdater::new(state, alias_contract_address)?;
    for contract_address in contract_addresses {
        if let Some(storage_keys) = contract_address_to_sorted_storage_keys.get(&contract_address) {
            for key in storage_keys {
                alias_updater.insert_alias(key)?;
            }
        }
        alias_updater.insert_alias(&StorageKey(contract_address.0))?;
    }

    let alias_storage_updates = alias_updater.finalize_updates();
    state_diff.storage.extend(alias_storage_updates);
    Ok(state_diff)
}

/// Generate updates for the alias contract with the new keys.
struct AliasUpdater<'a, S: StateReader> {
    state: &'a S,
    new_aliases: HashMap<AliasKey, Alias>,
    next_free_alias: Option<Alias>,
    alias_contract_address: ContractAddress,
}

impl<'a, S: StateReader> AliasUpdater<'a, S> {
    fn new(state: &'a S, alias_contract_address: ContractAddress) -> StateResult<Self> {
        let stored_counter =
            state.get_storage_at(alias_contract_address, ALIAS_COUNTER_STORAGE_KEY)?;
        Ok(Self {
            state,
            new_aliases: HashMap::new(),
            next_free_alias: if stored_counter == Felt::ZERO { None } else { Some(stored_counter) },
            alias_contract_address,
        })
    }

    /// Inserts the alias key to the updates if it's not already aliased.
    fn insert_alias(&mut self, alias_key: &AliasKey) -> StateResult<()> {
        if alias_key.0 >= MIN_VALUE_FOR_ALIAS_ALLOC
            && self.state.get_storage_at(self.alias_contract_address, *alias_key)? == Felt::ZERO
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
            .map(|(key, alias)| ((self.alias_contract_address, key), alias))
            .collect()
    }
}

/// Compresses the state diff by replacing the addresses and storage keys with aliases.
pub fn compress<S: StateReader>(
    state_diff: &StateMaps,
    state: &S,
    alias_contract_address: ContractAddress,
) -> CompressionResult<StateMaps> {
    let alias_compressor = AliasCompressor { state, alias_contract_address };

    let mut nonces = HashMap::new();
    for (contract_address, nonce) in state_diff.nonces.iter() {
        nonces.insert(alias_compressor.compress_address(contract_address)?, *nonce);
    }
    let mut class_hashes = HashMap::new();
    for (contract_address, class_hash) in state_diff.class_hashes.iter() {
        class_hashes.insert(alias_compressor.compress_address(contract_address)?, *class_hash);
    }
    let mut storage = HashMap::new();
    for ((contract_address, key), value) in state_diff.storage.iter() {
        storage.insert(
            (
                alias_compressor.compress_address(contract_address)?,
                alias_compressor.compress_storage_key(key, contract_address)?,
            ),
            *value,
        );
    }

    Ok(StateMaps { nonces, class_hashes, storage, ..state_diff.clone() })
}

/// Replaces contact addresses and storage keys with aliases.
struct AliasCompressor<'a, S: StateReader> {
    state: &'a S,
    alias_contract_address: ContractAddress,
}

impl<S: StateReader> AliasCompressor<'_, S> {
    fn compress_address(
        &self,
        contract_address: &ContractAddress,
    ) -> CompressionResult<ContractAddress> {
        if contract_address.0 >= MIN_VALUE_FOR_ALIAS_ALLOC {
            Ok(self.get_alias(StorageKey(contract_address.0))?.try_into()?)
        } else {
            Ok(*contract_address)
        }
    }

    fn compress_storage_key(
        &self,
        storage_key: &StorageKey,
        contact_address: &ContractAddress,
    ) -> CompressionResult<StorageKey> {
        if storage_key.0 >= MIN_VALUE_FOR_ALIAS_ALLOC
            && contact_address > &MAX_NON_COMPRESSED_CONTRACT_ADDRESS
        {
            Ok(self.get_alias(*storage_key)?.try_into()?)
        } else {
            Ok(*storage_key)
        }
    }

    fn get_alias(&self, alias_key: AliasKey) -> CompressionResult<Alias> {
        let alias = self.state.get_storage_at(self.alias_contract_address, alias_key)?;
        if alias == Felt::ZERO { Err(CompressionError::MissedAlias(alias_key)) } else { Ok(alias) }
    }
}
