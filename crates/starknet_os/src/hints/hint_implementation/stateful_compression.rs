use std::collections::HashMap;

use blockifier::state::state_api::{State, StateReader};
use cairo_vm::any_box;
use cairo_vm::hint_processor::builtin_hint_processor::dict_manager::DictManager;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    insert_value_from_var_name,
    insert_value_into_ap,
};

use crate::hints::error::HintResult;
use crate::hints::types::HintArgs;
use crate::hints::vars::{CairoStruct, Const, Ids, Scope};
use crate::vm_utils::get_address_of_nested_fields;

pub(crate) fn enter_scope_with_aliases<S: StateReader>(
    HintArgs { exec_scopes, .. }: HintArgs<'_, S>,
) -> HintResult {
    // Note that aliases, execution_helper and os_input do not enter the new scope as they are not
    // needed.
    let dict_manager_str: &str = Scope::DictManager.into();
    let dict_manager: DictManager = exec_scopes.get(dict_manager_str)?;
    let new_scope = HashMap::from([(dict_manager_str.to_string(), any_box!(dict_manager))]);
    exec_scopes.enter_scope(new_scope);
    Ok(())
}

pub(crate) fn get_alias_entry_for_state_update<S: StateReader>(
    HintArgs { vm, ids_data, ap_tracking, constants, exec_scopes, hint_processor }: HintArgs<'_, S>,
) -> HintResult {
    let aliases_contract_address = Const::AliasContractAddress.fetch(constants)?;
    let nested_fields = vec!["contract_state_changes_end".to_string()];

    let dict_ptr = get_address_of_nested_fields(
        ids_data,
        Ids::OsStateUpdate,
        CairoStruct::OsStateUpdate,
        vm,
        ap_tracking,
        &nested_fields,
        &hint_processor.execution_helper.os_program,
    )?;

    let dict_manager = exec_scopes.get_dict_manager()?;
    let mut dict_manager_borrowed = dict_manager.borrow_mut();
    let alias_entry = dict_manager_borrowed
        .get_tracker_mut(dict_ptr)?
        .get_value(&aliases_contract_address.into())?;

    Ok(insert_value_from_var_name(
        Ids::AliasesEntry.into(),
        alias_entry,
        vm,
        ids_data,
        ap_tracking,
    )?)
}

pub(crate) fn key_lt_min_alias_alloc_value<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn assert_key_big_enough_for_alias<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn read_alias_from_key<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn write_next_alias_from_key<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn read_alias_counter<S: StateReader>(
    HintArgs { hint_processor, vm, constants, .. }: HintArgs<'_, S>,
) -> HintResult {
    let aliases_contract_address = Const::get_alias_contract_address(constants)?;
    let alias_counter_storage_key = Const::get_alias_counter_storage_key(constants)?;
    let alias_counter = hint_processor
        .execution_helper
        .cached_state
        .get_storage_at(aliases_contract_address, alias_counter_storage_key)?;
    Ok(insert_value_into_ap(vm, alias_counter)?)
}

pub(crate) fn initialize_alias_counter<S: StateReader>(
    HintArgs { hint_processor, constants, .. }: HintArgs<'_, S>,
) -> HintResult {
    let aliases_contract_address = Const::get_alias_contract_address(constants)?;
    let alias_counter_storage_key = Const::get_alias_counter_storage_key(constants)?;
    let initial_available_alias = *Const::InitialAvailableAlias.fetch(constants)?;
    Ok(hint_processor.execution_helper.cached_state.set_storage_at(
        aliases_contract_address,
        alias_counter_storage_key,
        initial_available_alias,
    )?)
}

pub(crate) fn update_alias_counter<S: StateReader>(
    HintArgs { hint_processor, constants, ids_data, ap_tracking, vm, .. }: HintArgs<'_, S>,
) -> HintResult {
    let aliases_contract_address = Const::get_alias_contract_address(constants)?;
    let alias_counter_storage_key = Const::get_alias_counter_storage_key(constants)?;
    let next_available_alias =
        get_integer_from_var_name(Ids::NextAvailableAlias.into(), vm, ids_data, ap_tracking)?;
    Ok(hint_processor.execution_helper.cached_state.set_storage_at(
        aliases_contract_address,
        alias_counter_storage_key,
        next_available_alias,
    )?)
}

pub(crate) fn contract_address_le_max_for_compression<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn compute_commitments_on_finalized_state_with_aliases<S: StateReader>(
    HintArgs { hint_processor, exec_scopes, .. }: HintArgs<'static, S>,
) -> HintResult {
    // TODO(Nimrod): Consider moving this hint to `state.rs`.
    exec_scopes.insert_value(
        Scope::CommitmentInfoByAddress.into(),
        &hint_processor.execution_helper.os_input.address_to_storage_commitment_info,
    );

    Ok(())
}
