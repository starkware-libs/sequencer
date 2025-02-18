use std::collections::HashMap;

use blockifier::state::state_api::{State, StateReader};
use blockifier::state::stateful_compression::{ALIAS_COUNTER_STORAGE_KEY, INITIAL_AVAILABLE_ALIAS};
use cairo_vm::any_box;
use cairo_vm::hint_processor::builtin_hint_processor::dict_manager::DictManager;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    get_ptr_from_var_name,
    insert_value_from_var_name,
    insert_value_into_ap,
};
use cairo_vm::vm::errors::hint_errors::HintError;
use starknet_api::core::ContractAddress;

use crate::hints::error::HintResult;
use crate::hints::types::HintArgs;
use crate::hints::vars::{Const, Ids, Scope};

pub(crate) fn enter_scope_with_aliases(
    HintArgs { exec_scopes, .. }: HintArgs<'_, '_, '_, '_, '_, '_>,
) -> HintResult {
    // Note that aliases, execution_helper and os_input do not enter the new scope as they are not
    // needed.
    let dict_manager_str: &str = Scope::DictManager.into();
    let dict_manager: DictManager = exec_scopes.get(dict_manager_str)?;
    let new_scope = HashMap::from([(dict_manager_str.to_string(), any_box!(dict_manager))]);
    exec_scopes.enter_scope(new_scope);
    Ok(())
}

pub(crate) fn get_alias_entry_for_state_update(
    HintArgs { vm, ids_data, ap_tracking, constants, exec_scopes, .. }: HintArgs<
        '_,
        '_,
        '_,
        '_,
        '_,
        '_,
    >,
) -> HintResult {
    let contract_state_str = Ids::ContractStateChanges.into();
    let aliases_contract_address = Const::AliasContractAddress.fetch(constants)?;

    let dict_ptr = get_ptr_from_var_name(contract_state_str, vm, ids_data, ap_tracking)?;
    let dict_manager = exec_scopes.get_dict_manager()?;
    let mut dict_manager_borrowed = dict_manager.borrow_mut();
    let dict_tracker = dict_manager_borrowed.get_tracker_mut(dict_ptr)?;
    let alias_entry = dict_tracker.get_value(&aliases_contract_address.into())?;

    insert_value_from_var_name(contract_state_str, alias_entry, vm, ids_data, ap_tracking)
}

pub(crate) fn key_lt_min_alias_alloc_value(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_>,
) -> HintResult {
    todo!()
}

pub(crate) fn assert_key_big_enough_for_alias(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_>,
) -> HintResult {
    todo!()
}

pub(crate) fn read_alias_from_key(HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_>) -> HintResult {
    todo!()
}

pub(crate) fn write_next_alias_from_key(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_>,
) -> HintResult {
    todo!()
}

pub(crate) fn read_alias_counter(
    HintArgs { hint_processor, vm, constants, .. }: HintArgs<'_, '_, '_, '_, '_, '_>,
) -> HintResult {
    let aliases_contract_address_as_felt = Const::AliasContractAddress.fetch(constants)?;
    let aliases_contract_address = ContractAddress::try_from(aliases_contract_address_as_felt)
        .expect("Failed to convert the alias contract address 0x2 to contract address.");
    let alias_counter = hint_processor
        .execution_helper
        .cached_state
        .get_storage_at(aliases_contract_address, ALIAS_COUNTER_STORAGE_KEY)
        .map_err(|_| HintError::CustomHint("Failed to read from storage.".into()))?;
    insert_value_into_ap(vm, alias_counter)
}

pub(crate) fn initialize_alias_counter(
    HintArgs { hint_processor, constants, .. }: HintArgs<'_, '_, '_, '_, '_, '_>,
) -> HintResult {
    let aliases_contract_address_as_felt = Const::AliasContractAddress.fetch(constants)?;
    let aliases_contract_address = ContractAddress::try_from(aliases_contract_address_as_felt)
        .expect("Failed to convert the alias contract address 0x2 to contract address.");
    hint_processor
        .execution_helper
        .cached_state
        .set_storage_at(
            aliases_contract_address,
            ALIAS_COUNTER_STORAGE_KEY,
            INITIAL_AVAILABLE_ALIAS,
        )
        .map_err(|_| HintError::CustomHint("Failed to write to storage.".into()))
}

pub(crate) fn update_alias_counter(
    HintArgs { hint_processor, constants, ids_data, ap_tracking, vm, .. }: HintArgs<
        '_,
        '_,
        '_,
        '_,
        '_,
        '_,
    >,
) -> HintResult {
    let aliases_contract_address_as_felt = Const::AliasContractAddress.fetch(constants)?;
    let aliases_contract_address = ContractAddress::try_from(aliases_contract_address_as_felt)
        .expect("Failed to convert the alias contract address 0x2 to contract address.");
    let alias_counter =
        get_integer_from_var_name(Ids::NextAvailableAlias.into(), vm, ids_data, ap_tracking)?;
    hint_processor
        .execution_helper
        .cached_state
        .set_storage_at(aliases_contract_address, ALIAS_COUNTER_STORAGE_KEY, alias_counter)
        .map_err(|_| HintError::CustomHint("Failed to write to storage.".into()))
}

pub(crate) fn contract_address_le_max_for_compression(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_>,
) -> HintResult {
    todo!()
}

pub(crate) fn compute_commitments_on_finalized_state_with_aliases(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_>,
) -> HintResult {
    todo!()
}
