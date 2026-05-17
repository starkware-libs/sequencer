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

#[rstest]
#[case::empty(StateMaps::default(), StateDiff::default())]
#[case::nonces(
    {
        let mut state_maps = StateMaps::default();
        state_maps.nonces.insert(address(1), Nonce(Felt::from(7u64)));
        state_maps
    },
    StateDiff {
        address_to_nonce: [(address(1), Nonce(Felt::from(7u64)))].into(),
        ..Default::default()
    }
)]
#[case::storage(
    {
        let mut state_maps = StateMaps::default();
        state_maps.storage.insert((address(1), StorageKey::from(5u32)), Felt::from(100u64));
        state_maps
    },
    StateDiff {
        storage_updates: [(
            address(1),
            [(StarknetStorageKey(StorageKey::from(5u32)), StarknetStorageValue(Felt::from(100u64)))]
                .into(),
        )]
        .into(),
        ..Default::default()
    }
)]
#[case::compiled_class_hashes(
    {
        let mut state_maps = StateMaps::default();
        state_maps
            .compiled_class_hashes
            .insert(ClassHash(Felt::from(0x42u64)), CompiledClassHash(Felt::from(0x99u64)));
        state_maps
    },
    StateDiff {
        class_hash_to_compiled_class_hash: [(
            ClassHash(Felt::from(0x42u64)),
            CommitterCompiledClassHash(Felt::from(0x99u64)),
        )]
        .into(),
        ..Default::default()
    }
)]
#[case::class_hashes(
    {
        let mut state_maps = StateMaps::default();
        state_maps.class_hashes.insert(address(1), ClassHash(Felt::from(0x42u64)));
        state_maps
    },
    StateDiff {
        address_to_class_hash: [(address(1), ClassHash(Felt::from(0x42u64)))].into(),
        ..Default::default()
    }
)]
#[case::declared_contracts_dropped(
    {
        let mut state_maps = StateMaps::default();
        state_maps.declared_contracts.insert(ClassHash(Felt::from(0x42u64)), true);
        state_maps
    },
    StateDiff::default()
)]
fn test_convert_state_maps(#[case] state_maps: StateMaps, #[case] expected_state_diff: StateDiff) {
    assert_eq!(state_maps_to_committer_state_diff(state_maps), expected_state_diff);
}
