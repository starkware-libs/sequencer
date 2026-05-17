use assert_matches::assert_matches;
use blockifier::state::cached_state::StateMaps;
use rstest::rstest;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
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

fn address(value: u64) -> ContractAddress {
    ContractAddress::try_from(Felt::from(value)).unwrap()
}

fn state_diff_with_storage_value(value: Felt) -> StateDiff {
    StateDiff {
        storage_updates: [(
            address(1),
            [(StarknetStorageKey(StorageKey::from(1u32)), StarknetStorageValue(value))].into(),
        )]
        .into(),
        ..Default::default()
    }
}

#[rstest]
#[case::empty(StateDiff::default())]
#[case::nonzero_storage(state_diff_with_storage_value(Felt::from(42u64)))]
#[case::nonce_update(StateDiff {
    address_to_nonce: [(address(1), Nonce(Felt::from(1u64)))].into(),
    ..Default::default()
})]
fn test_validate_accepts_valid_state_diff(#[case] state_diff: StateDiff) {
    validate_virtual_os_state_diff(&state_diff).unwrap();
}

#[rstest]
#[case::storage_deletion("Storage deletion", state_diff_with_storage_value(Felt::ZERO))]
#[case::contract_deployment("Contract deployments", StateDiff {
    address_to_class_hash: [(address(1), ClassHash(Felt::from(0x42u64)))].into(),
    ..Default::default()
})]
#[case::contract_declaration("Contract declarations", StateDiff {
    class_hash_to_compiled_class_hash: [(
        ClassHash(Felt::from(0x42u64)),
        CommitterCompiledClassHash(Felt::from(0x99u64)),
    )]
    .into(),
    ..Default::default()
})]
fn test_validate_rejects_invalid_state_diff(
    #[case] expected_error_substring: &str,
    #[case] state_diff: StateDiff,
) {
    assert_matches!(
        validate_virtual_os_state_diff(&state_diff).unwrap_err(),
        ProofProviderError::InvalidStateDiff(message) if message.contains(expected_error_substring)
    );
}

#[test]
fn test_convert_empty_state_maps() {
    let state_diff = state_maps_to_committer_state_diff(StateMaps::default());

    assert!(state_diff.address_to_class_hash.is_empty());
    assert!(state_diff.address_to_nonce.is_empty());
    assert!(state_diff.class_hash_to_compiled_class_hash.is_empty());
    assert!(state_diff.storage_updates.is_empty());
}

#[test]
fn test_convert_state_maps_preserves_nonces() {
    let contract = address(1);
    let nonce = Nonce(Felt::from(7u64));
    let mut state_maps = StateMaps::default();
    state_maps.nonces.insert(contract, nonce);

    let state_diff = state_maps_to_committer_state_diff(state_maps);

    assert_eq!(state_diff.address_to_nonce, [(contract, nonce)].into());
}

#[test]
fn test_convert_state_maps_preserves_storage() {
    let contract = address(1);
    let storage_key = StorageKey::from(5u32);
    let storage_value = Felt::from(100u64);
    let mut state_maps = StateMaps::default();
    state_maps.storage.insert((contract, storage_key), storage_value);

    let state_diff = state_maps_to_committer_state_diff(state_maps);

    let contract_storage = state_diff.storage_updates.get(&contract).unwrap();
    assert_eq!(
        contract_storage.get(&StarknetStorageKey(storage_key)),
        Some(&StarknetStorageValue(storage_value))
    );
}

#[test]
fn test_convert_state_maps_preserves_compiled_class_hashes() {
    let class_hash = ClassHash(Felt::from(0x42u64));
    let compiled_class_hash = CompiledClassHash(Felt::from(0x99u64));
    let mut state_maps = StateMaps::default();
    state_maps.compiled_class_hashes.insert(class_hash, compiled_class_hash);

    let state_diff = state_maps_to_committer_state_diff(state_maps);

    assert_eq!(
        state_diff.class_hash_to_compiled_class_hash,
        [(class_hash, CommitterCompiledClassHash(compiled_class_hash.0))].into(),
    );
}
