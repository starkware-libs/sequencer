use std::collections::{HashMap, HashSet};

use starknet_api::core::ContractAddress;
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use crate::state::cached_state::StateMaps;
use crate::state::state_api::StateReader;
use crate::state::stateful_compression::{
    Alias,
    AliasKey,
    MAX_NON_COMPRESSED_CONTRACT_ADDRESS,
    MIN_VALUE_FOR_ALIAS_ALLOC,
};

/// Decompresses the state diff by replacing the aliases with addresses and storage keys.
pub fn decompress<S: StateReader>(
    state_diff: &StateMaps,
    state: &S,
    alias_contract_address: ContractAddress,
    alias_keys: HashSet<AliasKey>,
) -> StateMaps {
    let alias_decompressor = AliasDecompressorUtil::new(state, alias_contract_address, alias_keys);

    let mut nonces = HashMap::new();
    for (alias_contract_address, nonce) in state_diff.nonces.iter() {
        nonces.insert(alias_decompressor.decompress_address(alias_contract_address), *nonce);
    }
    let mut class_hashes = HashMap::new();
    for (alias_contract_address, class_hash) in state_diff.class_hashes.iter() {
        class_hashes
            .insert(alias_decompressor.decompress_address(alias_contract_address), *class_hash);
    }
    let mut storage = HashMap::new();
    for ((alias_contract_address, alias_storage_key), value) in state_diff.storage.iter() {
        let contract_address = alias_decompressor.decompress_address(alias_contract_address);
        storage.insert(
            (
                contract_address,
                alias_decompressor.decompress_storage_key(alias_storage_key, &contract_address),
            ),
            *value,
        );
    }

    StateMaps { nonces, class_hashes, storage, ..state_diff.clone() }
}

/// Replaces aliases with the original contact addresses and storage keys.
pub(crate) struct AliasDecompressorUtil {
    reversed_alias_mapping: HashMap<Alias, AliasKey>,
}

impl AliasDecompressorUtil {
    fn new<S: StateReader>(
        state: &S,
        alias_contract_address: ContractAddress,
        alias_keys: HashSet<AliasKey>,
    ) -> Self {
        let mut reversed_alias_mapping = HashMap::new();
        for alias_key in alias_keys.into_iter() {
            reversed_alias_mapping.insert(
                state.get_storage_at(alias_contract_address, alias_key).unwrap(),
                alias_key,
            );
        }
        Self { reversed_alias_mapping }
    }

    fn decompress_address(&self, contract_address_alias: &ContractAddress) -> ContractAddress {
        if contract_address_alias.0 >= MIN_VALUE_FOR_ALIAS_ALLOC {
            ContractAddress::try_from(
                *self.restore_alias_key(Felt::from(*contract_address_alias)).key(),
            )
            .unwrap()
        } else {
            *contract_address_alias
        }
    }

    fn decompress_storage_key(
        &self,
        storage_key_alias: &StorageKey,
        contact_address: &ContractAddress,
    ) -> StorageKey {
        if storage_key_alias.0 >= MIN_VALUE_FOR_ALIAS_ALLOC
            && contact_address > &MAX_NON_COMPRESSED_CONTRACT_ADDRESS
        {
            self.restore_alias_key(*storage_key_alias.0)
        } else {
            *storage_key_alias
        }
    }

    fn restore_alias_key(&self, alias: Alias) -> AliasKey {
        *self.reversed_alias_mapping.get(&alias).unwrap()
    }
}
