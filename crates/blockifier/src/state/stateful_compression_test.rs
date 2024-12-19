use std::collections::HashMap;
use std::sync::LazyLock;

use rstest::rstest;
use starknet_api::core::{ClassHash, ContractAddress, PatriciaKey};
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use super::{
    state_diff_with_alias_allocation,
    AliasUpdater,
    ALIAS_COUNTER_STORAGE_KEY,
    INITIAL_AVAILABLE_ALIAS,
    MAX_NON_COMPRESSED_CONTRACT_ADDRESS,
};
use crate::state::cached_state::{CachedState, StorageEntry};
use crate::state::state_api::{State, StateReader};
use crate::test_utils::dict_state_reader::DictStateReader;

static ALIAS_CONTRACT_ADDRESS: LazyLock<ContractAddress> =
    LazyLock::new(|| ContractAddress(PatriciaKey::try_from(Felt::TWO).unwrap()));

fn insert_to_alias_contract(
    storage: &mut HashMap<StorageEntry, Felt>,
    key: StorageKey,
    value: Felt,
) {
    storage.insert((*ALIAS_CONTRACT_ADDRESS, key), value);
}

fn initial_state(n_existing_aliases: u8) -> CachedState<DictStateReader> {
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
    let state = initial_state(n_existing_aliases);

    // Insert the keys into the alias contract updater and finalize the updates.
    let mut alias_contract_updater = AliasUpdater::new(&state, *ALIAS_CONTRACT_ADDRESS).unwrap();
    for key in keys {
        alias_contract_updater.insert_alias(&StorageKey::try_from(key).unwrap()).unwrap();
    }
    let storage_diff = alias_contract_updater.finalize_updates();

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
        .set_storage_at(
            *MAX_NON_COMPRESSED_CONTRACT_ADDRESS,
            StorageKey::from(0x301_u16),
            Felt::ONE,
        )
        .unwrap();
    state.get_class_hash_at(ContractAddress::from(0x202_u16)).unwrap();
    state.set_class_hash_at(ContractAddress::from(0x202_u16), ClassHash(Felt::ONE)).unwrap();
    state.increment_nonce(ContractAddress::from(0x200_u16)).unwrap();

    let storage_diff =
        state_diff_with_alias_allocation(&mut state, *ALIAS_CONTRACT_ADDRESS).unwrap().storage;
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
            ((*MAX_NON_COMPRESSED_CONTRACT_ADDRESS, StorageKey::from(0x301_u16)), Felt::ONE),
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
    let storage_diff =
        state_diff_with_alias_allocation(&mut state, *ALIAS_CONTRACT_ADDRESS).unwrap().storage;

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
