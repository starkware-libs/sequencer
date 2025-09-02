use std::collections::HashMap;

use indexmap::IndexMap;
use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_api::state::CommitmentStateDiff;
use starknet_committer::block_committer::input::{ConfigImpl, Input};
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia_storage::errors::DeserializationError;
use starknet_patricia_storage::map_storage::MapStorage;
use starknet_patricia_storage::storage_trait::{DbKey, DbValue};
use starknet_types_core::felt::Felt;

use crate::committer_cli::parse_input::raw_input::RawInput;

pub type InputImpl = Input<ConfigImpl>;

#[derive(Debug, PartialEq)]
pub struct CommitterInputImpl {
    pub input: InputImpl,
    pub storage: MapStorage,
}

impl TryFrom<RawInput> for CommitterInputImpl {
    type Error = DeserializationError;
    fn try_from(raw_input: RawInput) -> Result<Self, Self::Error> {
        let mut storage = HashMap::new();
        for entry in raw_input.storage {
            add_unique_hashmap(&mut storage, "storage", DbKey(entry.key), DbValue(entry.value))?;
        }

        let mut address_to_class_hash = IndexMap::new();
        for entry in raw_input.state_diff.address_to_class_hash {
            add_unique_indexmap(
                &mut address_to_class_hash,
                "address to class hash",
                ContractAddress::try_from(Felt::from_bytes_be_slice(&entry.key))?,
                ClassHash(Felt::from_bytes_be_slice(&entry.value)),
            )?;
        }

        let mut address_to_nonce = IndexMap::new();
        for entry in raw_input.state_diff.address_to_nonce {
            add_unique_indexmap(
                &mut address_to_nonce,
                "address to nonce",
                ContractAddress::try_from(Felt::from_bytes_be_slice(&entry.key))?,
                Nonce(Felt::from_bytes_be_slice(&entry.value)),
            )?;
        }

        let mut class_hash_to_compiled_class_hash = IndexMap::new();
        for entry in raw_input.state_diff.class_hash_to_compiled_class_hash {
            add_unique_indexmap(
                &mut class_hash_to_compiled_class_hash,
                "class hash to compiled class hash",
                ClassHash(Felt::from_bytes_be_slice(&entry.key)),
                starknet_api::core::CompiledClassHash(Felt::from_bytes_be_slice(&entry.value)),
            )?;
        }

        let mut storage_updates = IndexMap::new();
        for outer_entry in raw_input.state_diff.storage_updates {
            let inner_map: IndexMap<
                starknet_api::state::StorageKey,
                starknet_types_core::felt::Felt,
            > = outer_entry
                .storage_updates
                .iter()
                .map(|inner_entry| {
                    Ok((
                        Felt::from_bytes_be_slice(&inner_entry.key).try_into()?,
                        Felt::from_bytes_be_slice(&inner_entry.value),
                    ))
                })
                .collect::<Result<_, Self::Error>>()?;
            add_unique_indexmap(
                &mut storage_updates,
                "starknet storage updates",
                ContractAddress::try_from(Felt::from_bytes_be_slice(&outer_entry.address))?,
                inner_map,
            )?;
        }
        let input = Input {
            state_diff: CommitmentStateDiff {
                address_to_class_hash,
                address_to_nonce,
                class_hash_to_compiled_class_hash,
                storage_updates,
            },
            contracts_trie_root_hash: HashOutput(Felt::from_bytes_be_slice(
                &raw_input.contracts_trie_root_hash,
            )),
            classes_trie_root_hash: HashOutput(Felt::from_bytes_be_slice(
                &raw_input.classes_trie_root_hash,
            )),
            config: raw_input.config.into(),
        };
        Ok(Self { input, storage })
    }
}

pub(crate) fn add_unique_hashmap<K, V>(
    map: &mut HashMap<K, V>,
    map_name: &str,
    key: K,
    value: V,
) -> Result<(), DeserializationError>
where
    K: std::cmp::Eq + std::hash::Hash + std::fmt::Debug,
{
    if map.contains_key(&key) {
        return Err(DeserializationError::KeyDuplicate(format!("{map_name}: {key:?}")));
    }
    map.insert(key, value);
    Ok(())
}

pub(crate) fn add_unique_indexmap<K, V>(
    map: &mut IndexMap<K, V>,
    map_name: &str,
    key: K,
    value: V,
) -> Result<(), DeserializationError>
where
    K: std::cmp::Eq + std::hash::Hash + std::fmt::Debug,
{
    if map.contains_key(&key) {
        return Err(DeserializationError::KeyDuplicate(format!("{map_name}: {key:?}")));
    }
    map.insert(key, value);
    Ok(())
}
