use std::collections::HashMap;

use rstest::rstest;
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use super::{
    AliasUpdater,
    ALIAS_CONTRACT_ADDRESS,
    ALIAS_COUNTER_STORAGE_KEY,
    MIN_VALUE_FOR_ALIAS_ALLOC,
};
use crate::state::cached_state::{CachedState, StorageEntry};
use crate::test_utils::dict_state_reader::DictStateReader;

fn insert_to_alias_contract(
    storage: &mut HashMap<StorageEntry, Felt>,
    key: StorageKey,
    value: Felt,
) {
    storage.insert((ALIAS_CONTRACT_ADDRESS, key), value);
}

fn initial_state(n_existing_aliases: u8) -> CachedState<DictStateReader> {
    let mut state_reader = DictStateReader::default();
    if n_existing_aliases > 0 {
        let high_alias_key = MIN_VALUE_FOR_ALIAS_ALLOC * Felt::TWO;
        insert_to_alias_contract(
            &mut state_reader.storage_view,
            ALIAS_COUNTER_STORAGE_KEY,
            MIN_VALUE_FOR_ALIAS_ALLOC + Felt::from(n_existing_aliases),
        );
        for i in 0..n_existing_aliases {
            insert_to_alias_contract(
                &mut state_reader.storage_view,
                (high_alias_key + Felt::from(i)).try_into().unwrap(),
                MIN_VALUE_FOR_ALIAS_ALLOC + Felt::from(i),
            );
        }
    }

    CachedState::new(state_reader)
}

/// Tests the alias contract updater with an empty state.
#[rstest]
#[case::no_update(vec![], vec![])]
#[case::low_update(vec![MIN_VALUE_FOR_ALIAS_ALLOC - 1], vec![])]
#[case::single_update(vec![MIN_VALUE_FOR_ALIAS_ALLOC], vec![MIN_VALUE_FOR_ALIAS_ALLOC])]
#[case::some_update(
    vec![
        MIN_VALUE_FOR_ALIAS_ALLOC + 1,
        MIN_VALUE_FOR_ALIAS_ALLOC - 1,
        MIN_VALUE_FOR_ALIAS_ALLOC,
        MIN_VALUE_FOR_ALIAS_ALLOC + 2,
        MIN_VALUE_FOR_ALIAS_ALLOC,
    ],
    vec![
        MIN_VALUE_FOR_ALIAS_ALLOC + 1,
        MIN_VALUE_FOR_ALIAS_ALLOC,
        MIN_VALUE_FOR_ALIAS_ALLOC + 2,
    ]
)]
fn test_alias_updater(
    #[case] keys: Vec<Felt>,
    #[case] expected_alias_keys: Vec<Felt>,
    #[values(0, 2)] n_existing_aliases: u8,
) {
    let mut state = initial_state(n_existing_aliases);

    // Insert the keys into the alias contract updater and finalize the updates.
    let mut alias_contract_updater = AliasUpdater::new(&mut state).unwrap();
    for key in keys {
        alias_contract_updater.insert_alias(&StorageKey::try_from(key).unwrap()).unwrap();
    }
    let storage_diff = alias_contract_updater.finalize_updates();

    // Test the new aliases.
    let mut expected_storage_diff = HashMap::new();
    let mut expected_next_alias = MIN_VALUE_FOR_ALIAS_ALLOC + Felt::from(n_existing_aliases);
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
