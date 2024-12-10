use std::collections::HashMap;

use starknet_committer::block_committer::input::{
    ConfigImpl,
    ContractAddress,
    Input,
    StarknetStorageKey,
    StarknetStorageValue,
    StateDiff,
};
use starknet_committer::patricia_merkle_tree::types::{ClassHash, CompiledClassHash, Nonce};
use starknet_patricia::felt::Felt;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia::storage::errors::DeserializationError;
use starknet_patricia::storage::storage_trait::{StorageKey, StorageValue};

use crate::parse_input::raw_input::RawInput;

pub type InputImpl = Input<ConfigImpl>;

impl TryFrom<RawInput> for InputImpl {
    type Error = DeserializationError;
    fn try_from(raw_input: RawInput) -> Result<Self, Self::Error> {
        let mut storage = HashMap::new();
        for entry in raw_input.storage {
            add_unique(&mut storage, "storage", StorageKey(entry.key), StorageValue(entry.value))?;
        }

        let mut address_to_class_hash = HashMap::new();
        for entry in raw_input.state_diff.address_to_class_hash {
            add_unique(
                &mut address_to_class_hash,
                "address to class hash",
                ContractAddress(Felt::from_bytes_be_slice(&entry.key)),
                ClassHash(Felt::from_bytes_be_slice(&entry.value)),
            )?;
        }

        let mut address_to_nonce = HashMap::new();
        for entry in raw_input.state_diff.address_to_nonce {
            add_unique(
                &mut address_to_nonce,
                "address to nonce",
                ContractAddress(Felt::from_bytes_be_slice(&entry.key)),
                Nonce(Felt::from_bytes_be_slice(&entry.value)),
            )?;
        }

        let mut class_hash_to_compiled_class_hash = HashMap::new();
        for entry in raw_input.state_diff.class_hash_to_compiled_class_hash {
            add_unique(
                &mut class_hash_to_compiled_class_hash,
                "class hash to compiled class hash",
                ClassHash(Felt::from_bytes_be_slice(&entry.key)),
                CompiledClassHash(Felt::from_bytes_be_slice(&entry.value)),
            )?;
        }

        let mut storage_updates = HashMap::new();
        for outer_entry in raw_input.state_diff.storage_updates {
            let inner_map = outer_entry
                .storage_updates
                .iter()
                .map(|inner_entry| {
                    (
                        StarknetStorageKey(Felt::from_bytes_be_slice(&inner_entry.key)),
                        StarknetStorageValue(Felt::from_bytes_be_slice(&inner_entry.value)),
                    )
                })
                .collect();
            add_unique(
                &mut storage_updates,
                "starknet storage updates",
                ContractAddress(Felt::from_bytes_be_slice(&outer_entry.address)),
                inner_map,
            )?;
        }

        Ok(Input {
            storage,
            state_diff: StateDiff {
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
        })
    }
}

pub(crate) fn add_unique<K, V>(
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
