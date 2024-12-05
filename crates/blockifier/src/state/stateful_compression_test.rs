use std::collections::HashMap;

use rstest::rstest;
use starknet_api::core::ContractAddress;
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use super::{
    get_alias_contract_address,
    get_alias_counter_storage_key,
    AliasUpdater,
    MIN_VALUE_FOR_ALIAS_ALLOC,
};
use crate::state::cached_state::CachedState;
use crate::test_utils::dict_state_reader::DictStateReader;

fn insert_to_alias_contract(
    storage: &mut HashMap<(ContractAddress, StorageKey), Felt>,
    key: StorageKey,
    value: Felt,
) {
    storage.insert((get_alias_contract_address(), key), value);
}

fn initial_state(n_exist_aliases: u8) -> CachedState<DictStateReader> {
    let mut state_reader = DictStateReader::default();
    if n_exist_aliases > 0 {
        let high_alias_key = MIN_VALUE_FOR_ALIAS_ALLOC * Felt::TWO;
        insert_to_alias_contract(
            &mut state_reader.storage_view,
            get_alias_counter_storage_key(),
            MIN_VALUE_FOR_ALIAS_ALLOC + Felt::from(n_exist_aliases),
        );
        for i in 0..n_exist_aliases {
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
    #[values(0, 2)] n_exist_aliases: u8,
) {
    let mut state = initial_state(n_exist_aliases);

    // Insert the keys into the alias contract updater and finalize the updates.
    let mut alias_contract_updater = AliasUpdater::new(&mut state).unwrap();
    for key in keys {
        alias_contract_updater.set_alias(&StorageKey::try_from(key).unwrap()).unwrap();
    }
    alias_contract_updater.finalize_updates().unwrap();
    let storage_diff = state.to_state_diff().unwrap().state_maps.storage;

    // Test the new aliases.
    let mut expected_storage_diff = HashMap::new();
    let mut next_alias = MIN_VALUE_FOR_ALIAS_ALLOC + Felt::from(n_exist_aliases);
    for key in &expected_alias_keys {
        insert_to_alias_contract(
            &mut expected_storage_diff,
            StorageKey::try_from(*key).unwrap(),
            next_alias,
        );
        next_alias += Felt::ONE;
    }
    if !expected_alias_keys.is_empty() || n_exist_aliases == 0 {
        insert_to_alias_contract(
            &mut expected_storage_diff,
            get_alias_counter_storage_key(),
            next_alias,
        );
    }

    assert_eq!(storage_diff, expected_storage_diff);
}
