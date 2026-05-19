use std::collections::BTreeSet;

use starknet_api::block::BlockNumber;
use starknet_api::core::{ContractAddress, Nonce, BLOCK_HASH_TABLE_ADDRESS};
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use super::AccessedKeys;
use crate::execution::call_info::{CallInfo, StorageAccessTracker};
use crate::execution::entry_point::CallEntryPoint;
use crate::state::cached_state::StateMaps;
use crate::state::stateful_compression::ALIAS_COUNTER_STORAGE_KEY;
use crate::test_utils::ALIAS_CONTRACT_ADDRESS;
use crate::transaction::objects::TransactionExecutionInfo;

fn call_info_with_storage_accesses(
    storage_address: ContractAddress,
    accessed_storage_keys: impl IntoIterator<Item = StorageKey>,
) -> CallInfo {
    CallInfo {
        call: CallEntryPoint { storage_address, ..Default::default() },
        storage_access_tracker: StorageAccessTracker {
            accessed_storage_keys: accessed_storage_keys.into_iter().collect(),
            ..Default::default()
        },
        ..Default::default()
    }
}

#[test]
fn no_inputs_no_alias_prediction_yields_empty_storage_keys() {
    let accessed = AccessedKeys::new(
        std::iter::empty::<&TransactionExecutionInfo>(),
        std::iter::empty::<&BlockNumber>(),
        &StateMaps::default(),
        *ALIAS_CONTRACT_ADDRESS,
        false,
    );
    assert!(accessed.storage_keys.is_empty());
    assert!(accessed.accessed_contracts.is_empty());
    assert!(accessed.accessed_class_hashes.is_empty());
}

#[test]
fn empty_inputs_with_alias_prediction_contains_counter_only() {
    let alias_contract_address = *ALIAS_CONTRACT_ADDRESS;
    let accessed = AccessedKeys::new(
        std::iter::empty::<&TransactionExecutionInfo>(),
        std::iter::empty::<&BlockNumber>(),
        &StateMaps::default(),
        alias_contract_address,
        true,
    );
    assert_eq!(
        accessed.storage_keys,
        BTreeSet::from([(alias_contract_address, ALIAS_COUNTER_STORAGE_KEY)]),
    );
    assert_eq!(accessed.accessed_contracts, BTreeSet::from([alias_contract_address]));
}

#[test]
fn state_diff_storage_keys_are_included() {
    let address = ContractAddress::from(0x100_u16);
    let storage_key = StorageKey::from(0x200_u16);
    let mut state_diff = StateMaps::default();
    state_diff.storage.insert((address, storage_key), Felt::ONE);

    let accessed = AccessedKeys::new(
        std::iter::empty::<&TransactionExecutionInfo>(),
        std::iter::empty::<&BlockNumber>(),
        &state_diff,
        *ALIAS_CONTRACT_ADDRESS,
        false,
    );
    assert!(accessed.storage_keys.contains(&(address, storage_key)));
    assert!(accessed.accessed_contracts.contains(&address));
}

#[test]
fn proof_facts_block_numbers_map_to_block_hash_table_entries() {
    let block_number_x = BlockNumber(42);
    let block_number_y = BlockNumber(99);
    let accessed = AccessedKeys::new(
        std::iter::empty::<&TransactionExecutionInfo>(),
        [&block_number_x, &block_number_y],
        &StateMaps::default(),
        *ALIAS_CONTRACT_ADDRESS,
        false,
    );
    assert!(
        accessed
            .storage_keys
            .contains(&(BLOCK_HASH_TABLE_ADDRESS, StorageKey::from(block_number_x.0)))
    );
    assert!(
        accessed
            .storage_keys
            .contains(&(BLOCK_HASH_TABLE_ADDRESS, StorageKey::from(block_number_y.0)))
    );
    assert!(accessed.accessed_contracts.contains(&BLOCK_HASH_TABLE_ADDRESS));
}

#[test]
fn alias_predictions_toggle_controls_alias_contract_entries() {
    let modified_address = ContractAddress::from(0x100_u16);
    let mut state_diff = StateMaps::default();
    state_diff.nonces.insert(modified_address, Nonce(Felt::ONE));
    let alias_contract_address = *ALIAS_CONTRACT_ADDRESS;

    let with_predictions = AccessedKeys::new(
        std::iter::empty::<&TransactionExecutionInfo>(),
        std::iter::empty::<&BlockNumber>(),
        &state_diff,
        alias_contract_address,
        true,
    );
    let without_predictions = AccessedKeys::new(
        std::iter::empty::<&TransactionExecutionInfo>(),
        std::iter::empty::<&BlockNumber>(),
        &state_diff,
        alias_contract_address,
        false,
    );
    assert!(
        with_predictions
            .storage_keys
            .contains(&(alias_contract_address, ALIAS_COUNTER_STORAGE_KEY))
    );
    assert!(
        without_predictions
            .storage_keys
            .iter()
            .all(|(address, _)| address != &alias_contract_address)
    );
}

#[test]
fn visited_storage_entries_from_execution_info_are_included() {
    let storage_address = ContractAddress::from(0x300_u16);
    let key_a = StorageKey::from(0x400_u16);
    let key_b = StorageKey::from(0x401_u16);
    let execution_info = TransactionExecutionInfo {
        execute_call_info: Some(call_info_with_storage_accesses(storage_address, [key_a, key_b])),
        ..Default::default()
    };

    let accessed = AccessedKeys::new(
        [&execution_info],
        std::iter::empty::<&BlockNumber>(),
        &StateMaps::default(),
        *ALIAS_CONTRACT_ADDRESS,
        false,
    );
    assert!(accessed.storage_keys.contains(&(storage_address, key_a)));
    assert!(accessed.storage_keys.contains(&(storage_address, key_b)));
    assert!(accessed.accessed_contracts.contains(&storage_address));
}

/// For a reverted invoke, `execute_call_info` is `None`; the validate and fee_transfer call infos
/// still exist and their accessed storage keys must be collected.
#[test]
fn reverted_invoke_collects_validate_and_fee_transfer_entries() {
    let validate_address = ContractAddress::from(0x500_u16);
    let validate_key = StorageKey::from(0x501_u16);
    let fee_address = ContractAddress::from(0x600_u16);
    let fee_key = StorageKey::from(0x601_u16);
    let execution_info = TransactionExecutionInfo {
        validate_call_info: Some(call_info_with_storage_accesses(validate_address, [validate_key])),
        execute_call_info: None,
        fee_transfer_call_info: Some(call_info_with_storage_accesses(fee_address, [fee_key])),
        ..Default::default()
    };

    let accessed = AccessedKeys::new(
        [&execution_info],
        std::iter::empty::<&BlockNumber>(),
        &StateMaps::default(),
        *ALIAS_CONTRACT_ADDRESS,
        false,
    );
    assert!(accessed.storage_keys.contains(&(validate_address, validate_key)));
    assert!(accessed.storage_keys.contains(&(fee_address, fee_key)));
}
