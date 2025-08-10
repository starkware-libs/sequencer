use std::collections::HashMap;

use assert_matches::assert_matches;
use rstest::rstest;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::felt;
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use super::{
    allocate_aliases_in_storage,
    compress,
    AliasUpdater,
    ALIAS_COUNTER_STORAGE_KEY,
    INITIAL_AVAILABLE_ALIAS,
    MAX_NON_COMPRESSED_CONTRACT_ADDRESS,
};
use crate::state::cached_state::{CachedState, StateMaps, StorageEntry};
use crate::state::state_api::{State, StateReader};
use crate::state::stateful_compression::{AliasCompressor, CompressionError};
use crate::state::stateful_compression_test_utils::decompress;
use crate::test_utils::dict_state_reader::DictStateReader;
use crate::test_utils::ALIAS_CONTRACT_ADDRESS;

pub(crate) fn insert_to_alias_contract(
    storage: &mut HashMap<StorageEntry, Felt>,
    key: StorageKey,
    value: Felt,
) {
    storage.insert((*ALIAS_CONTRACT_ADDRESS, key), value);
}

pub(crate) fn initial_state(n_existing_aliases: u8) -> CachedState<DictStateReader> {
    let mut state_reader = DictStateReader::default();
    if n_existing_aliases > 0 {
        let high_alias_key = INITIAL_AVAILABLE_ALIAS * Felt::TWO;
        insert_to_alias_contract(
            &mut state_reader.storage_view,
            ALIAS_COUNTER_STORAGE_KEY,
            INITIAL_AVAILABLE_ALIAS + Felt::from(n_existing_aliases),
        );
        for i in 0..n_existing_aliases {
            insert_to_alias_contract(
                &mut state_reader.storage_view,
                (high_alias_key + Felt::from(i)).try_into().unwrap(),
                INITIAL_AVAILABLE_ALIAS + Felt::from(i),
            );
        }
    }

    CachedState::new(state_reader)
}

/// Tests the alias contract updater with an empty state.
#[rstest]
#[case::no_update(vec![], vec![])]
#[case::low_update(vec![INITIAL_AVAILABLE_ALIAS - 1], vec![])]
#[case::single_update(vec![INITIAL_AVAILABLE_ALIAS], vec![INITIAL_AVAILABLE_ALIAS])]
#[case::some_update(
    vec![
        INITIAL_AVAILABLE_ALIAS + 1,
        INITIAL_AVAILABLE_ALIAS - 1,
        INITIAL_AVAILABLE_ALIAS,
        INITIAL_AVAILABLE_ALIAS + 2,
        INITIAL_AVAILABLE_ALIAS,
    ],
    vec![
        INITIAL_AVAILABLE_ALIAS + 1,
        INITIAL_AVAILABLE_ALIAS,
        INITIAL_AVAILABLE_ALIAS + 2,
    ]
)]
fn test_alias_updater(
    #[case] keys: Vec<Felt>,
    #[case] expected_alias_keys: Vec<Felt>,
    #[values(0, 2)] n_existing_aliases: u8,
) {
    let mut state = initial_state(n_existing_aliases);

    // Insert the keys into the alias contract updater and finalize the updates.
    let mut alias_contract_updater =
        AliasUpdater::new(&mut state, *ALIAS_CONTRACT_ADDRESS).unwrap();
    for key in keys {
        alias_contract_updater.insert_alias(&StorageKey::try_from(key).unwrap()).unwrap();
    }
    alias_contract_updater.finalize_updates().unwrap();
    let storage_diff = state.to_state_diff().unwrap().state_maps.storage;

    // Test the new aliases.
    let mut expected_storage_diff = HashMap::new();
    let mut expected_next_alias = INITIAL_AVAILABLE_ALIAS + Felt::from(n_existing_aliases);
    for key in &expected_alias_keys {
        insert_to_alias_contract(
            &mut expected_storage_diff,
            StorageKey::try_from(*key).unwrap(),
            expected_next_alias,
        );
        expected_next_alias += Felt::ONE;
    }
    if !expected_alias_keys.is_empty() || n_existing_aliases == 0 {
        insert_to_alias_contract(
            &mut expected_storage_diff,
            ALIAS_COUNTER_STORAGE_KEY,
            expected_next_alias,
        );
    }

    assert_eq!(storage_diff, expected_storage_diff);
}

#[test]
fn test_iterate_aliases() {
    let mut state = initial_state(0);
    state
        .set_storage_at(ContractAddress::from(0x201_u16), StorageKey::from(0x307_u16), Felt::ONE)
        .unwrap();
    state
        .set_storage_at(ContractAddress::from(0x201_u16), StorageKey::from(0x309_u16), Felt::TWO)
        .unwrap();
    state
        .set_storage_at(ContractAddress::from(0x201_u16), StorageKey::from(0x304_u16), Felt::THREE)
        .unwrap();
    state
        .set_storage_at(MAX_NON_COMPRESSED_CONTRACT_ADDRESS, StorageKey::from(0x301_u16), Felt::ONE)
        .unwrap();
    state.get_class_hash_at(ContractAddress::from(0x202_u16)).unwrap();
    state.set_class_hash_at(ContractAddress::from(0x202_u16), ClassHash(Felt::ONE)).unwrap();
    state.increment_nonce(ContractAddress::from(0x200_u16)).unwrap();

    allocate_aliases_in_storage(&mut state, *ALIAS_CONTRACT_ADDRESS).unwrap();
    let storage_diff = state.to_state_diff().unwrap().state_maps.storage;

    assert_eq!(
        storage_diff,
        vec![
            (
                (*ALIAS_CONTRACT_ADDRESS, ALIAS_COUNTER_STORAGE_KEY),
                INITIAL_AVAILABLE_ALIAS + Felt::from(6_u8)
            ),
            ((*ALIAS_CONTRACT_ADDRESS, StorageKey::from(0x200_u16)), INITIAL_AVAILABLE_ALIAS),
            (
                (*ALIAS_CONTRACT_ADDRESS, StorageKey::from(0x304_u16)),
                INITIAL_AVAILABLE_ALIAS + Felt::ONE
            ),
            (
                (*ALIAS_CONTRACT_ADDRESS, StorageKey::from(0x307_u16)),
                INITIAL_AVAILABLE_ALIAS + Felt::TWO
            ),
            (
                (*ALIAS_CONTRACT_ADDRESS, StorageKey::from(0x309_u16)),
                INITIAL_AVAILABLE_ALIAS + Felt::THREE
            ),
            (
                (*ALIAS_CONTRACT_ADDRESS, StorageKey::from(0x201_u16)),
                INITIAL_AVAILABLE_ALIAS + Felt::from(4_u8)
            ),
            (
                (*ALIAS_CONTRACT_ADDRESS, StorageKey::from(0x202_u16)),
                INITIAL_AVAILABLE_ALIAS + Felt::from(5_u8)
            ),
            ((ContractAddress::from(0x201_u16), StorageKey::from(0x304_u16)), Felt::THREE),
            ((ContractAddress::from(0x201_u16), StorageKey::from(0x307_u16)), Felt::ONE),
            ((ContractAddress::from(0x201_u16), StorageKey::from(0x309_u16)), Felt::TWO),
            ((MAX_NON_COMPRESSED_CONTRACT_ADDRESS, StorageKey::from(0x301_u16)), Felt::ONE),
        ]
        .into_iter()
        .collect()
    );
}

#[rstest]
fn test_read_only_state(#[values(0, 2)] n_existing_aliases: u8) {
    let mut state = initial_state(n_existing_aliases);
    state
        .set_storage_at(ContractAddress::from(0x200_u16), StorageKey::from(0x300_u16), Felt::ZERO)
        .unwrap();
    state.get_nonce_at(ContractAddress::from(0x201_u16)).unwrap();
    state.get_class_hash_at(ContractAddress::from(0x202_u16)).unwrap();
    allocate_aliases_in_storage(&mut state, *ALIAS_CONTRACT_ADDRESS).unwrap();
    let storage_diff = state.to_state_diff().unwrap().state_maps.storage;

    let expected_storage_diff = if n_existing_aliases == 0 {
        HashMap::from([(
            (*ALIAS_CONTRACT_ADDRESS, ALIAS_COUNTER_STORAGE_KEY),
            INITIAL_AVAILABLE_ALIAS,
        )])
    } else {
        HashMap::new()
    };
    assert_eq!(storage_diff, expected_storage_diff);
}

/// Tests the range of alias keys that should be compressed.
#[test]
fn test_alias_compressor() {
    let alias = Felt::from(500_u16);

    let high_key = 200_u16;
    let high_storage_key = StorageKey::from(high_key);
    let high_contract_address = ContractAddress::from(high_key);

    let no_aliasing_key = 50_u16;
    let no_aliasing_storage_key = StorageKey::from(no_aliasing_key);
    let no_aliasing_contract_address = ContractAddress::from(no_aliasing_key);

    let no_compression_contract_address = ContractAddress::from(10_u16);

    let mut state_reader = DictStateReader::default();
    insert_to_alias_contract(&mut state_reader.storage_view, high_storage_key, alias);
    let alias_compressor =
        AliasCompressor { state: &state_reader, alias_contract_address: *ALIAS_CONTRACT_ADDRESS };

    assert_eq!(
        alias_compressor.compress_address(&high_contract_address).unwrap(),
        ContractAddress::try_from(alias).unwrap(),
    );
    assert_eq!(
        alias_compressor.compress_address(&no_aliasing_contract_address).unwrap(),
        no_aliasing_contract_address,
    );

    assert_eq!(
        alias_compressor.compress_storage_key(&high_storage_key, &high_contract_address).unwrap(),
        StorageKey::try_from(alias).unwrap(),
    );
    assert_eq!(
        alias_compressor
            .compress_storage_key(&no_aliasing_storage_key, &high_contract_address)
            .unwrap(),
        no_aliasing_storage_key,
    );
    assert_eq!(
        alias_compressor
            .compress_storage_key(&high_storage_key, &no_compression_contract_address)
            .unwrap(),
        high_storage_key,
    );

    let missed_key = 300_u16;
    let err = alias_compressor.compress_address(&ContractAddress::from(missed_key));
    assert_matches!(
        err,
        Err(CompressionError::MissedAlias(key)) if key == missed_key.into()
    );
}

#[test]
fn test_compression() {
    let state_reader = DictStateReader {
        storage_view: (200_u16..206)
            .map(|x| ((*ALIAS_CONTRACT_ADDRESS, StorageKey::from(x)), Felt::from(x + 100)))
            .collect(),
        ..Default::default()
    };

    // State diff with values that should not be compressed.
    let base_state_diff = StateMaps {
        nonces: vec![(ContractAddress::from(30_u16), Nonce(Felt::ONE))].into_iter().collect(),
        class_hashes: vec![(ContractAddress::from(31_u16), ClassHash(Felt::ONE))]
            .into_iter()
            .collect(),
        storage: vec![((ContractAddress::from(10_u16), StorageKey::from(205_u16)), Felt::TWO)]
            .into_iter()
            .collect(),
        compiled_class_hashes: vec![(ClassHash(felt!("0x400")), CompiledClassHash(felt!("0x401")))]
            .into_iter()
            .collect(),
        declared_contracts: vec![(ClassHash(felt!("0x402")), true)].into_iter().collect(),
    };

    let compressed_base_state_diff =
        compress(&base_state_diff, &state_reader, *ALIAS_CONTRACT_ADDRESS).unwrap();
    assert_eq!(compressed_base_state_diff, base_state_diff);

    // Add to the state diff values that should be compressed.
    let mut state_diff = base_state_diff.clone();
    state_diff.extend(&StateMaps {
        nonces: vec![(ContractAddress::from(200_u16), Nonce(Felt::ZERO))].into_iter().collect(),
        class_hashes: vec![(ContractAddress::from(201_u16), ClassHash(Felt::ZERO))]
            .into_iter()
            .collect(),
        storage: vec![
            ((ContractAddress::from(202_u16), StorageKey::from(203_u16)), Felt::ZERO),
            ((ContractAddress::from(32_u16), StorageKey::from(204_u16)), Felt::ONE),
        ]
        .into_iter()
        .collect(),
        ..Default::default()
    });

    let mut expected_compressed_state_diff = base_state_diff.clone();
    expected_compressed_state_diff.extend(&StateMaps {
        nonces: vec![(ContractAddress::from(300_u16), Nonce(Felt::ZERO))].into_iter().collect(),
        class_hashes: vec![(ContractAddress::from(301_u16), ClassHash(Felt::ZERO))]
            .into_iter()
            .collect(),
        storage: vec![
            ((ContractAddress::from(302_u16), StorageKey::from(303_u16)), Felt::ZERO),
            ((ContractAddress::from(32_u16), StorageKey::from(304_u16)), Felt::ONE),
        ]
        .into_iter()
        .collect(),
        ..Default::default()
    });

    let compressed_state_diff =
        compress(&state_diff, &state_reader, *ALIAS_CONTRACT_ADDRESS).unwrap();
    assert_eq!(compressed_state_diff, expected_compressed_state_diff);

    let alias_keys = state_reader.storage_view.keys().map(|(_, key)| key.0).collect();
    let decompressed_state_diff =
        decompress(&compressed_state_diff, &state_reader, *ALIAS_CONTRACT_ADDRESS, alias_keys);
    assert_eq!(decompressed_state_diff, state_diff);
}
