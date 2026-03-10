use std::collections::HashMap;

use assert_matches::assert_matches;
use blockifier::state::cached_state::StateMaps;
use rstest::rstest;
use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_api::state::StorageKey;
use starknet_committer::block_committer::input::{
    StarknetStorageKey,
    StarknetStorageValue,
    StateDiff,
};
use starknet_committer::patricia_merkle_tree::types::CompiledClassHash as CommitterCompiledClassHash;
use starknet_types_core::felt::Felt;

use crate::errors::ProofProviderError;
use crate::running::committer_utils::{
    state_maps_to_committer_state_diff,
    validate_virtual_os_state_diff,
};

fn make_contract_address(value: u64) -> ContractAddress {
    ContractAddress::try_from(Felt::from(value)).unwrap()
}

// validate_virtual_os_state_diff tests

#[test]
fn test_validate_empty_state_diff() {
    let state_diff = StateDiff {
        address_to_class_hash: Default::default(),
        address_to_nonce: Default::default(),
        class_hash_to_compiled_class_hash: Default::default(),
        storage_updates: Default::default(),
    };
    assert!(validate_virtual_os_state_diff(&state_diff).is_ok());
}

#[test]
fn test_validate_state_diff_with_valid_storage_updates() {
    let address = make_contract_address(1);
    let storage_key = StarknetStorageKey(StorageKey::from(1u32));
    let non_zero_value = StarknetStorageValue(Felt::from(42u64));

    let mut address_storage: HashMap<StarknetStorageKey, StarknetStorageValue> = HashMap::new();
    address_storage.insert(storage_key, non_zero_value);

    let mut storage_updates: HashMap<
        ContractAddress,
        HashMap<StarknetStorageKey, StarknetStorageValue>,
    > = HashMap::new();
    storage_updates.insert(address, address_storage);

    let state_diff = StateDiff {
        address_to_class_hash: Default::default(),
        address_to_nonce: Default::default(),
        class_hash_to_compiled_class_hash: Default::default(),
        storage_updates,
    };
    assert!(validate_virtual_os_state_diff(&state_diff).is_ok());
}

#[test]
fn test_validate_state_diff_with_valid_nonce_updates() {
    let address = make_contract_address(1);
    let storage_key = StarknetStorageKey(StorageKey::from(1u32));
    let non_zero_value = StarknetStorageValue(Felt::from(42u64));

    let mut address_storage: HashMap<StarknetStorageKey, StarknetStorageValue> = HashMap::new();
    address_storage.insert(storage_key, non_zero_value);

    let mut storage_updates: HashMap<
        ContractAddress,
        HashMap<StarknetStorageKey, StarknetStorageValue>,
    > = HashMap::new();
    storage_updates.insert(address, address_storage);

    let mut address_to_nonce = HashMap::new();
    address_to_nonce.insert(address, Nonce(Felt::from(1u64)));

    let state_diff = StateDiff {
        address_to_class_hash: Default::default(),
        address_to_nonce,
        class_hash_to_compiled_class_hash: Default::default(),
        storage_updates,
    };
    assert!(validate_virtual_os_state_diff(&state_diff).is_ok());
}

#[rstest]
#[case("Storage deletion", {
    let address = make_contract_address(1);
    let storage_key = StarknetStorageKey(StorageKey::from(1u32));
    let zero_value = StarknetStorageValue(Felt::ZERO);
    let mut address_storage: HashMap<StarknetStorageKey, StarknetStorageValue> = HashMap::new();
    address_storage.insert(storage_key, zero_value);
    let mut storage_updates: HashMap<ContractAddress, HashMap<StarknetStorageKey, StarknetStorageValue>> =
        HashMap::new();
    storage_updates.insert(address, address_storage);
    StateDiff {
        address_to_class_hash: Default::default(),
        address_to_nonce: Default::default(),
        class_hash_to_compiled_class_hash: Default::default(),
        storage_updates,
    }
})]
#[case("Contract deployments", {
    let address = make_contract_address(1);
    let class_hash = ClassHash(Felt::from(0x42u64));
    let mut address_to_class_hash = HashMap::new();
    address_to_class_hash.insert(address, class_hash);
    StateDiff {
        address_to_class_hash,
        address_to_nonce: Default::default(),
        class_hash_to_compiled_class_hash: Default::default(),
        storage_updates: Default::default(),
    }
})]
#[case("Contract declarations", {
    let class_hash = ClassHash(Felt::from(0x42u64));
    let compiled_class_hash = CommitterCompiledClassHash(Felt::from(0x99u64));
    let mut class_hash_to_compiled_class_hash = HashMap::new();
    class_hash_to_compiled_class_hash.insert(class_hash, compiled_class_hash);
    StateDiff {
        address_to_class_hash: Default::default(),
        address_to_nonce: Default::default(),
        class_hash_to_compiled_class_hash,
        storage_updates: Default::default(),
    }
})]
fn test_validate_rejects_invalid_state_diff(
    #[case] expected_error_substring: &str,
    #[case] state_diff: StateDiff,
) {
    let error = validate_virtual_os_state_diff(&state_diff).unwrap_err();
    assert_matches!(
        &error,
        ProofProviderError::InvalidStateDiff(message) if message.contains(expected_error_substring)
    );
}

// state_maps_to_committer_state_diff tests

#[test]
fn test_convert_empty_state_maps() {
    let state_maps = StateMaps::default();
    let state_diff = state_maps_to_committer_state_diff(state_maps);

    assert!(state_diff.address_to_class_hash.is_empty());
    assert!(state_diff.address_to_nonce.is_empty());
    assert!(state_diff.class_hash_to_compiled_class_hash.is_empty());
    assert!(state_diff.storage_updates.is_empty());
}

#[test]
fn test_convert_state_maps_preserves_nonces() {
    let address = make_contract_address(1);
    let nonce = Nonce(Felt::from(7u64));

    let mut state_maps = StateMaps::default();
    state_maps.nonces.insert(address, nonce);

    let state_diff = state_maps_to_committer_state_diff(state_maps);

    assert_eq!(state_diff.address_to_nonce.get(&address), Some(&nonce));
    assert_eq!(state_diff.address_to_nonce.len(), 1);
}

#[test]
fn test_convert_state_maps_preserves_storage() {
    let address = make_contract_address(1);
    let storage_key = StorageKey::from(5u32);
    let storage_value = Felt::from(100u64);

    let mut state_maps = StateMaps::default();
    state_maps.storage.insert((address, storage_key), storage_value);

    let state_diff = state_maps_to_committer_state_diff(state_maps);

    let committer_key = StarknetStorageKey(storage_key);
    let committer_value = StarknetStorageValue(storage_value);

    let address_storage =
        state_diff.storage_updates.get(&address).expect("Address should be in storage_updates");
    assert_eq!(address_storage.get(&committer_key), Some(&committer_value));
}

#[test]
fn test_convert_state_maps_preserves_compiled_class_hashes() {
    let class_hash = ClassHash(Felt::from(0x42u64));
    let compiled_class_hash = starknet_api::core::CompiledClassHash(Felt::from(0x99u64));

    let mut state_maps = StateMaps::default();
    state_maps.compiled_class_hashes.insert(class_hash, compiled_class_hash);

    let state_diff = state_maps_to_committer_state_diff(state_maps);

    let expected_committer_hash = CommitterCompiledClassHash(compiled_class_hash.0);
    assert_eq!(
        state_diff.class_hash_to_compiled_class_hash.get(&class_hash),
        Some(&expected_committer_hash)
    );
    assert_eq!(state_diff.class_hash_to_compiled_class_hash.len(), 1);
}
